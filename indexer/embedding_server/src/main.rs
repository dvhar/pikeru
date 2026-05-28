//! Embedding server for pikeru semantic search.
//!
//! Similar to the caption_server, but serves raw CLIP embeddings instead of text captions.
//! Images are encoded with a vision model; user queries are encoded with the matching
//! text model — both into the same shared vector space.
//!
//! Long-running HTTP server with /health, /embed/image, /embed/text endpoints.
//! The Python embed_indexer.py script is the CLI client that invokes this server.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use fastembed::{InitOptions, ImageEmbedding, TextEmbedding};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

// ── Shared state ────────────────────────────────────────────────────────────

struct AppState {
    image_encoder: ImageEmbedding,
    text_encoder: TextEmbedding,
}

#[derive(Deserialize)]
struct EmbedImageRequest {
    /// Filesystem path to an image (for direct HTTP calls).
    path: Option<String>,
    /// Base64-encoded image bytes + explicit extension.
    #[serde(default)]
    path_b64: Option<String>,
    /// Image file extension — required when using path_b64, ignored for path.
    ext: Option<String>,
}

/// Valid image extensions that fastembed/ort can decode.
const VALID_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff", "tif"];

#[derive(Deserialize)]
struct EmbedTextRequest {
    text: String,
}

#[derive(Serialize)]
struct EmbedResponse {
    dim: usize,
    #[serde(with = "serde_f32_array")]
    embedding: Vec<f32>,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
}

mod serde_f32_array {
    use serde::{self, Serializer};

    pub fn serialize<S>(vals: &[f32], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(vals.len()))?;
        for v in vals {
            seq.serialize_element(v)?;
        }
        seq.end()
    }
}

// ── Handlers ────────────────────────────────────────────────────────────────

async fn health(_state: State<Arc<Mutex<AppState>>>) -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

async fn embed_image(
    state: State<Arc<Mutex<AppState>>>,
    Json(req): Json<EmbedImageRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, String)> {
    let mut guard = state.0.lock().await;

    let file_path: String = match (&req.path, &req.path_b64) {
        (Some(p), _) => p.clone(),
        (None, Some(b64)) => {
            // Caller must provide an explicit extension for the temp file.
            let ext = req.ext.as_deref().ok_or_else(|| {
                (StatusCode::BAD_REQUEST, "path_b64 requires 'ext' field".into())
            })?;
            if !VALID_EXTS.contains(&ext.to_lowercase().as_str()) {
                return Err((StatusCode::BAD_REQUEST,
                    format!("unsupported extension '{}'", ext)));
            }
            let bytes = STANDARD.decode(b64).map_err(|e| {
                (StatusCode::BAD_REQUEST, format!("bad base64: {e}"))
            })?;
            let p = format!("/tmp/pikeru_emb_{}.{}", std::process::id(), ext);
            if std::fs::write(&p, &bytes).is_err() {
                return Err((StatusCode::INTERNAL_SERVER_ERROR,
                    "cannot write temp image".into()));
            }
            p
        }
        _ => return Err((StatusCode::BAD_REQUEST,
            "missing 'path' or 'path_b64' field".into())),
    };

    eprintln!("Embedding: {}", file_path);
    let embeddings = guard.image_encoder.embed(&[&file_path], None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR,
            format!("embedding failed: {e}")))?;
    if embeddings.is_empty() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "empty result".into()));
    }

    let emb = &embeddings[0];
    Ok(Json(EmbedResponse { dim: emb.len(), embedding: emb.clone() }))
}

async fn embed_text(
    state: State<Arc<Mutex<AppState>>>,
    Json(req): Json<EmbedTextRequest>,
) -> Result<Json<EmbedResponse>, (StatusCode, String)> {
    let mut guard = state.0.lock().await;
    let embeddings = guard.text_encoder.embed(&[&req.text], None)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR,
            format!("embedding failed: {e}")))?;
    if embeddings.is_empty() {
        return Err((StatusCode::INTERNAL_SERVER_ERROR, "embedding failed".into()));
    }

    let emb = &embeddings[0];
    Ok(Json(EmbedResponse { dim: emb.len(), embedding: emb.clone() }))
}

// ── Server ──────────────────────────────────────────────────────────────────

async fn run_server(addr: SocketAddr) -> Result<()> {
    let state = Arc::new(Mutex::new(AppState {
        image_encoder: ImageEmbedding::try_new(Default::default())
            .context("failed to load CLIP vision model")?,
        text_encoder: TextEmbedding::try_new(InitOptions::new(
            fastembed::EmbeddingModel::ClipVitB32,
        ))
        .context("failed to load CLIP text model")?,
    }));

    let app = Router::new()
        .route("/health", get(health))
        .route("/embed/image", post(embed_image))
        .route("/embed/text", post(embed_text))
        .with_state(state);

    eprintln!("embedding-server listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

// ── Entry point ─────────────────────────────────────────────────────────────

fn print_usage() {
    eprintln!("Usage: embedding-server --serve [--port PORT]");
    eprintln!("  Starts the HTTP embedding server (default port 6285).");
    eprintln!();
    eprintln!("Use embed_indexer.py as the CLI client to encode image files.");
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--serve") => {
            let port = args.iter().position(|a| a == "--port")
                .and_then(|i| args.get(i + 1).and_then(|p| p.parse::<u16>().ok()))
                .unwrap_or(6285);
            let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;
            run_server(addr).await
        }
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        _ => {
            print_usage();
            std::process::exit(1);
        }
    }
}
