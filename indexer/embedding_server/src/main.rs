//! Embedding server for pikeru semantic search — async TCP with fastembed CLIP.
//! Serves 512-dim CLIP embeddings for images and text.
//! Image uploads use multipart/form-data; text queries use JSON POST.

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use anyhow::Context;
use fastembed::{get_cache_dir, ImageEmbedding, ImageInitOptions, InitOptions, TextEmbedding};
use image::ImageFormat;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const HOST: &str = "127.0.0.1:6285";

// ── Shared state ────────────────────────────────────────────────────────────

struct AppState {
    image_encoder: StdMutex<ImageEmbedding>,
    text_encoder: StdMutex<TextEmbedding>,
}

// ── Response helpers ────────────────────────────────────────────────────────

/// Serialize f32 slice to little-endian bytes.
fn embed_to_bytes(embedding: &[f32]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(embedding.len() * 4);
    for &v in embedding {
        buf.extend_from_slice(&v.to_le_bytes());
    }
    buf
}

/// Minimal HTTP response builder. Always closes connection after response.
fn http_response(status: u16, status_text: &str, content_type: &str, body: &[u8]) -> Vec<u8> {
    let mut resp = Vec::new();
    resp.extend_from_slice(format!("HTTP/1.1 {} {}\r\n", status, status_text).as_bytes());
    resp.extend_from_slice(b"Content-Length: ");
    resp.extend_from_slice(body.len().to_string().as_bytes());
    resp.extend_from_slice(b"\r\n");
    resp.extend_from_slice(b"Content-Type: ");
    resp.extend_from_slice(content_type.as_bytes());
    resp.extend_from_slice(b"\r\n");
    resp.extend_from_slice(b"Connection: close\r\n");
    resp.extend_from_slice(b"\r\n");
    resp.extend_from_slice(body);
    resp
}

fn json_error(msg: &str) -> Vec<u8> {
    let body = serde_json::json!({"error": msg}).to_string();
    http_response(400, "Bad Request", "application/json", body.as_bytes())
}

// ── Byte-level helpers ──────────────────────────────────────────────────────

/// Find a byte pattern in a byte slice.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Find the byte offset of the header/body separator (\r\n\r\n) in raw bytes.
fn find_header_end(data: &[u8]) -> Option<usize> {
    const SEP: &[u8] = b"\r\n\r\n";
    for i in 0..=data.len().saturating_sub(SEP.len()) {
        if data[i..i + SEP.len()] == *SEP {
            return Some(i);
        }
    }
    None
}

// ── Multipart parser ────────────────────────────────────────────────────────

/// Parse multipart/form-data to extract image data and filename.
/// Uses byte-level splitting to handle binary image data that may not be valid UTF-8.
fn parse_multipart(body: &[u8], boundary: &str) -> anyhow::Result<(String, Vec<u8>)> {
    let full_boundary = format!("--{}", boundary);
    let sep_bytes = full_boundary.as_bytes();

    // Find all boundary positions
    let mut split_points = Vec::new();
    let mut search_start = 0;
    while let Some(pos) = body[search_start..].windows(sep_bytes.len()).position(|w| w == sep_bytes)
    {
        split_points.push(search_start + pos);
        search_start = search_start + pos + sep_bytes.len();
    }

    if split_points.is_empty() {
        anyhow::bail!("No boundary found in multipart data");
    }

    let mut filename = String::new();
    let mut image_data: Option<Vec<u8>> = None;

    for i in 0..split_points.len() {
        let part_start = split_points[i] + sep_bytes.len();
        let part_end = if i + 1 < split_points.len() {
            split_points[i + 1]
        } else {
            body.len()
        };

        if part_start >= part_end {
            continue;
        }

        let mut part = &body[part_start..part_end];

        // Strip trailing -- (closing boundary)
        if part.ends_with(b"--") {
            part = &part[..part.len() - 2];
        }
        // Trim leading/trailing \r\n
        part = part.strip_prefix(b"\r\n").unwrap_or(part);
        part = if part.ends_with(b"\r\n") {
            &part[..part.len() - 2]
        } else {
            part
        };

        if part.is_empty() {
            continue;
        }

        // Find \r\n\r\n (header/body separator) using byte search
        if let Some(header_end) = find_bytes(part, b"\r\n\r\n") {
            let header_part = &part[..header_end];

            // Check for filename= in the headers (byte-level search)
            if let Some(fnpos) = find_bytes(header_part, b"filename=") {
                let after = &header_part[fnpos + 9..];
                // Skip opening quote if present
                let after = if after.first() == Some(&b'"') {
                    &after[1..]
                } else {
                    after
                };
                let end_pos = after.iter().position(|&b| b == b'"' || b == b'\r' || b == b'\n');
                let end_pos = end_pos.unwrap_or(after.len());
                filename = String::from_utf8_lossy(&after[..end_pos]).to_string();
            }

            // Image data is everything after \r\n\r\n
            let data = part[header_end + 4..].to_vec();
            image_data = Some(data);
        }
    }

    if filename.is_empty() {
        anyhow::bail!("No filename found in multipart data");
    }
    let image_data =
        image_data.ok_or_else(|| anyhow::anyhow!("No image data found in multipart body"))?;

    Ok((filename, image_data))
}

// ── HTTP request parser ─────────────────────────────────────────────────────

/// Parse a simple HTTP request, returning (method, path, body, content_type, content_length).
/// Only the header portion is parsed as UTF-8; the body is kept as raw bytes.
fn parse_request(raw: &[u8]) -> anyhow::Result<(String, String, Vec<u8>, String, usize)> {
    let header_end = find_header_end(raw).context("No header separator found")?;
    let headers_end = header_end + 4;

    let header_text = std::str::from_utf8(&raw[..header_end])?;
    let mut lines = header_text.lines();

    let request_line = lines.next().context("Empty request")?;
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("Malformed request line: {}", request_line);
    }
    let method = parts[0].to_string();
    let path = parts[1].to_string();

    let mut content_type = String::new();
    let mut content_length = 0usize;
    for line in lines {
        if let Some(colon) = line.find(':') {
            let key = line[..colon].trim().to_lowercase();
            let val = line[colon + 1..].trim();
            match key.as_str() {
                "content-type" => content_type = val.to_string(),
                "content-length" => content_length = val.parse().unwrap_or(0),
                _ => {}
            }
        }
    }

    let body = raw[headers_end..].to_vec();
    Ok((method, path, body, content_type, content_length))
}

/// Read HTTP request from a stream: first the headers, then the body (based on Content-Length).
async fn read_request(socket: &mut tokio::net::TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut raw = Vec::new();
    loop {
        let mut chunk = vec![0u8; 8192];
        let n = socket.read(&mut chunk).await?;
        if n == 0 {
            break; // EOF
        }
        raw.extend_from_slice(&chunk[..n]);

        if let Some(header_end) = find_header_end(&raw) {
            let header_bytes = &raw[..header_end];
            if let Ok(text) = std::str::from_utf8(header_bytes) {
                let mut content_length = 0usize;
                for line in text.lines().skip(1) {
                    if let Some(colon) = line.find(':') {
                        let key = line[..colon].trim().to_lowercase();
                        let val = line[colon + 1..].trim();
                        if key == "content-length" {
                            content_length = val.parse().unwrap_or(0);
                        }
                    }
                }
                let body_start = header_end + 4;
                let already_read = raw.len().saturating_sub(body_start);
                let remaining = content_length.saturating_sub(already_read);
                if remaining > 0 {
                    let mut body_buf = vec![0u8; remaining];
                    socket.read_exact(&mut body_buf).await?;
                    raw.extend_from_slice(&body_buf);
                }
                break;
            }
        }
    }
    Ok(raw)
}

// ── Image embedding ─────────────────────────────────────────────────────────

const VALID_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"];

fn valid_ext(ext: &str) -> bool {
    VALID_EXTS.contains(&ext.to_lowercase().as_str())
}

/// Detect image format from bytes.
fn detect_image_format(bytes: &[u8]) -> Option<ImageFormat> {
    image::guess_format(bytes).ok()
}

async fn handle_image_request(
    state: Arc<AppState>,
    body: &[u8],
    content_type: &str,
) -> Vec<u8> {
    // Extract boundary from Content-Type
    let boundary = content_type
        .split("boundary=")
        .nth(1)
        .unwrap_or("----WebKitFormBoundary")
        .trim();

    let (filename, image_bytes) = match parse_multipart(body, boundary) {
        Ok(data) => data,
        Err(e) => return json_error(&format!("parse multipart: {}", e)),
    };

    if image_bytes.is_empty() {
        return json_error("empty image data");
    }

    // Validate extension
    let ext = filename
        .rsplit('.')
        .next()
        .map(|s| s.to_lowercase());
    if let Some(ref ext) = ext {
        if !valid_ext(ext) {
            return json_error(&format!("unsupported extension '{}'", ext));
        }
    }

    // Run embedding in a blocking task (CPU-intensive, needs &mut encoder)
    let fn_name = filename;
    let img_bytes = image_bytes;

    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<f32>> {
        let mut encoder = state.image_encoder.lock().map_err(|e| anyhow::anyhow!("mutex poison: {}", e))?;
        let tmp_path = create_tmp_image_path(&img_bytes, &fn_name)?;
        let embeddings = encoder.embed(&[tmp_path.as_path()], None)?;
        let emb = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty embedding result"))?;
        let _ = std::fs::remove_file(&tmp_path);
        Ok(emb)
    })
    .await;

    match result {
        Ok(Ok(embedding)) => {
            let bytes = embed_to_bytes(&embedding);
            http_response(200, "OK", "application/octet-stream", &bytes)
        }
        Ok(Err(e)) => json_error(&format!("embedding failed: {}", e)),
        Err(e) => json_error(&format!("task join error: {}", e)),
    }
}

/// Create a temp file path for an image, returning the path and writing data to it.
fn create_tmp_image_path(image_bytes: &[u8], filename: &str) -> anyhow::Result<PathBuf> {
    let pid = std::process::id();
    let ext = filename
        .rsplit('.')
        .next()
        .map(|s| s.to_lowercase())
        .filter(|s| valid_ext(s));

    let tmp_path = if let Some(ref ext) = ext {
        PathBuf::from(format!("/tmp/pikeru_emb_{pid}.{ext}"))
    } else {
        match detect_image_format(image_bytes) {
            Some(ImageFormat::Png) => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.png")),
            Some(ImageFormat::Jpeg) => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.jpg")),
            Some(ImageFormat::WebP) => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.webp")),
            Some(ImageFormat::Gif) => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.gif")),
            Some(ImageFormat::Tiff) => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.tif")),
            _ => PathBuf::from(format!("/tmp/pikeru_emb_{pid}.png")),
        }
    };

    std::fs::write(&tmp_path, image_bytes)
        .with_context(|| format!("failed to write temp file {}", tmp_path.display()))?;
    Ok(tmp_path)
}

// ── Text embedding ──────────────────────────────────────────────────────────

async fn handle_text_request(state: Arc<AppState>, body: &[u8]) -> Vec<u8> {
    #[derive(serde::Deserialize)]
    struct TextRequest {
        text: String,
    }

    let text = match serde_json::from_slice::<TextRequest>(body) {
        Ok(req) => req.text,
        Err(e) => return json_error(&format!("bad JSON: {}", e)),
    };

    if text.is_empty() {
        return json_error("text field is empty");
    }

    // Run embedding in a blocking task (CPU-intensive, needs &mut encoder)
    let text_owned = text.clone();
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<f32>> {
        let mut encoder = state.text_encoder.lock().map_err(|e| anyhow::anyhow!("mutex poison: {}", e))?;
        let embeddings = encoder.embed(&[&text_owned], None)?;
        let emb = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty embedding result"))?;
        Ok(emb)
    })
    .await;

    match result {
        Ok(Ok(embedding)) => {
            let bytes = embed_to_bytes(&embedding);
            http_response(200, "OK", "application/octet-stream", &bytes)
        }
        Ok(Err(e)) => json_error(&format!("embedding failed: {}", e)),
        Err(e) => json_error(&format!("task join error: {}", e)),
    }
}

// ── Server ──────────────────────────────────────────────────────────────────

/// Parse CLI arguments: --port PORT and --cache-dir DIR.
fn parse_cli_args(args: &[String]) -> anyhow::Result<(String, PathBuf)> {
    let mut host = HOST.to_string();
    let mut cache_dir: PathBuf = get_cache_dir().into();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                let port = args.get(i + 1)
                    .and_then(|p| p.parse::<u16>().ok())
                    .context("--port requires a numeric value")?;
                host = format!("127.0.0.1:{}", port);
                i += 2;
            }
            "--cache-dir" => {
                cache_dir = args
                    .get(i + 1)
                    .context("--cache-dir requires a path argument")?
                    .clone()
                    .into();
                i += 2;
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Usage: embedding-server [OPTIONS]");
                eprintln!("  --port PORT       Port to listen on (default: 6285)");
                eprintln!("  --cache-dir DIR   Directory to store downloaded models");
                std::process::exit(1);
            }
        }
    }

    Ok((host, cache_dir))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        eprintln!("Usage: embedding-server [OPTIONS]");
        eprintln!("Options:");
        eprintln!("  --port PORT       Port to listen on (default: 6285)");
        eprintln!("  --cache-dir DIR   Directory to store downloaded models");
        eprintln!("                   (default: $FASTEMBED_CACHE_DIR or .fastembed_cache)");
        return Ok(());
    }

    let (host, cache_dir) = parse_cli_args(&args)?;

    println!("Loading CLIP models...");
    let state: Arc<AppState> = Arc::new(AppState {
        image_encoder: StdMutex::new(
            ImageEmbedding::try_new(ImageInitOptions::default().with_cache_dir(cache_dir.clone()))
                .context("failed to load CLIP vision model")?,
        ),
        text_encoder: StdMutex::new(
            TextEmbedding::try_new(InitOptions::new(
                fastembed::EmbeddingModel::ClipVitB32,
            )
            .with_cache_dir(cache_dir))
            .context("failed to load CLIP text model")?,
        ),
    });
    println!("Models loaded.");

    let listener = TcpListener::bind(&host).await.context("failed to bind TCP socket")?;
    println!("Embedding server listening on {host}");

    loop {
        let (mut socket, _addr) = listener.accept().await?;
        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let raw = match read_request(&mut socket).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Read error: {e}");
                    return;
                }
            };

            let response = match parse_request(&raw) {
                Ok((method, path, body, content_type, _)) => {
                    match (method.as_str(), path.as_str()) {
                        ("GET", "/health") => {
                            let json = serde_json::json!({"status": "ok"}).to_string();
                            http_response(200, "OK", "application/json", json.as_bytes())
                        }
                        ("POST", "/embed/image") => {
                            handle_image_request(Arc::clone(&state), &body, &content_type).await
                        }
                        ("POST", "/embed/text") => handle_text_request(Arc::clone(&state), &body).await,
                        _ => {
                            let body = serde_json::json!({"error": "not found"}).to_string();
                            http_response(404, "Not Found", "application/json", body.as_bytes())
                        }
                    }
                }
                Err(e) => {
                    let body = serde_json::json!({"error": e.to_string()}).to_string();
                    http_response(400, "Bad Request", "application/json", body.as_bytes())
                }
            };

            if let Err(e) = socket.write_all(&response).await {
                eprintln!("Write error: {e}");
            }
        });
    }
}
