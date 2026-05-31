//! Shared test infrastructure for the xdg-desktop-portal-pikeru backend.
//! All test artifacts live under /tmp/pikeru_tests and are cleaned up automatically.
//! Tests use unique D-Bus service names (t0001, t0002, ...) to avoid collision
//! with the system-installed portal.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use rusqlite::Connection;

// ---------------------------------------------------------------------------
// Temp directory management
// ---------------------------------------------------------------------------

pub fn test_base_dir() -> PathBuf {
    let base = PathBuf::from("/tmp/pikeru_tests");
    std::fs::create_dir_all(&base).unwrap();
    base
}

pub fn test_run_dir() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = test_base_dir().join(format!("run_{}", ts));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn test_workspace() -> TempDir {
    let run = test_run_dir();
    let ws = run.join("workspace");
    std::fs::create_dir_all(&ws).unwrap();
    TempDir::new_in(ws.parent().unwrap()).unwrap()
}

// ---------------------------------------------------------------------------
// Mock file picker script — pure mock, never launches real pikeru
// ---------------------------------------------------------------------------

pub fn create_mock_wrapper(workspace: &TempDir, outputs: &[&str]) -> PathBuf {
    let wrapper = workspace.path().join("mock-pikeru-wrapper.sh");
    let mut script = String::from("#!/bin/bash\n# mock — does NOT invoke real pikeru\n");
    for output in outputs {
        script.push_str(&format!("echo \"{}\"\n", output));
    }
    std::fs::write(&wrapper, script).unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).unwrap();
    wrapper
}

#[allow(dead_code)]
pub fn create_mock_indexer(workspace: &TempDir, description: &str) -> PathBuf {
    let script = workspace.path().join("mock-indexer.sh");
    std::fs::write(&script, format!("#!/bin/bash\necho \"{}\"\nexit 0\n", description)).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

/// Creates a mock vector indexer that outputs binary data in the same format
/// as vector_indexer.py: 4-byte header (u16 LE dim, u16 LE bit_width) followed
/// by raw f32 vector data. The portal validates this header before storing.
///
/// The mock also records whether PK_INDEX_EMBEDDING is set (the portal sets it
/// when indexer mode is vector) to a capture file so tests can assert it.
#[allow(dead_code)]
pub fn create_mock_vector_indexer(workspace: &TempDir, num_floats: usize) -> PathBuf {
    let capture_file = workspace.path().join("vector_env_capture.txt");
    let capture_path = capture_file.to_str().unwrap();

    // Write the Python helper as a separate file to avoid shell quoting issues
    // with format strings like '<HH{}f' that bash would interpret as redirection.
    let py_script = workspace.path().join("mock-vector-indexer.py");

    // Build the Python script using string concatenation to avoid Rust format
    // string conflicts with Python's struct.pack format specifiers.
    let py_lines: Vec<String> = vec![
        "import struct, sys, os".into(),
        format!("with open('{}', 'a') as f:", capture_path),
        "    f.write('PK_INDEX_EMBEDDING=' + os.environ.get('PK_INDEX_EMBEDDING', '<unset>') + '\\n')".into(),
        format!("dim = {}", num_floats),
        "bit_width = 32".into(),
        "sys.stdout.buffer.write(struct.pack('HH', dim, bit_width))".into(),
        format!("vals = [0.1] * {}", num_floats),
        format!("data = struct.pack('<{}f', *vals)", num_floats),
        "sys.stdout.buffer.write(data)".into(),
    ];
    std::fs::write(&py_script, py_lines.join("\n")).unwrap();

    let wrapper = workspace.path().join("mock-vector-indexer.sh");
    std::fs::write(&wrapper, format!(
        "#!/bin/bash\npython3 '{}'\nexit 0\n",
        py_script.display()
    )).unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).unwrap();
    wrapper
}

/// Creates a mock text indexer that outputs a plain description string.
/// Records whether PK_INDEX_EMBEDDING was set to a capture file so tests
/// can assert it was NOT present during text indexing.
#[allow(dead_code)]
pub fn create_mock_text_indexer(workspace: &TempDir, description: &str) -> PathBuf {
    let capture_file = workspace.path().join("text_env_capture.txt");
    let capture_path = capture_file.to_str().unwrap();

    let wrapper = workspace.path().join("mock-text-indexer.sh");
    // Record env var status and output description.
    let env_log = format!("echo PK_INDEX_EMBEDDING=$PK_INDEX_EMBEDDING >> '{}'", capture_path);
    let script = format!(
        "#!/bin/bash\n{}\necho '{}'",
        env_log, description
    );
    std::fs::write(&wrapper, script).unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).unwrap();
    wrapper
}

/// Creates a mock query indexer that receives the text as a command-line
/// argument (just like a filepath during indexing), records whether
/// PK_QUERY_EMBEDDING was set, and outputs a valid binary embedding.
/// Used for testing the TextEmbed D-Bus method.
#[allow(dead_code)]
pub fn create_mock_query_indexer(workspace: &TempDir, num_floats: usize) -> PathBuf {
    let capture_file = workspace.path().join("query_env_capture.txt");
    let capture_path = capture_file.to_str().unwrap();

    let py_script = workspace.path().join("mock-query-indexer.py");
    let py_lines: Vec<String> = vec![
        "import struct, sys, os".into(),
        format!("with open('{}', 'w') as f:", capture_path),
        "    f.write('PK_QUERY_EMBEDDING=' + os.environ.get('PK_QUERY_EMBEDDING', '<unset>') + '\\n')".into(),
        "# Text is passed as a command-line argument (like a filepath)".into(),
        "_text = sys.argv[1] if len(sys.argv) > 1 else ''".into(),
        format!("dim = {}", num_floats),
        "bit_width = 32".into(),
        "sys.stdout.buffer.write(struct.pack('HH', dim, bit_width))".into(),
        format!("vals = [0.5] * {}", num_floats),
        format!("data = struct.pack('<{}f', *vals)", num_floats),
        "sys.stdout.buffer.write(data)".into(),
    ];
    std::fs::write(&py_script, py_lines.join("\n")).unwrap();

    let wrapper = workspace.path().join("mock-query-indexer.sh");
    std::fs::write(&wrapper, format!(
        "#!/bin/bash\npython3 '{}' '$1'\n", py_script.display()
    )).unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).unwrap();
    wrapper
}

#[allow(dead_code)]
/// Creates a mock wrapper that sleeps for approximately `delay_ms` milliseconds
/// per invocation (using Python for sub-second precision), then echoes the
/// argument. Useful for timing-based tests where you want indexing to be slow
/// enough that other operations (like clear_queue) can interrupt between
/// directory processing.
#[allow(dead_code)]
pub fn create_slow_mock_wrapper(workspace: &TempDir, delay_ms: u64, outputs: &[&str]) -> PathBuf {
    let script = workspace.path().join("slow-mock-wrapper.sh");
    let mut lines: Vec<String> = vec!["#!/bin/bash".to_string()];
    if delay_ms > 0 {
        let sleep_secs = (delay_ms as f64) / 1000.0;
        lines.push(format!("python3 -c 'import time; time.sleep({})'", sleep_secs));
    }
    for output in outputs {
        lines.push(format!("echo \"{}\"", output));
    }
    std::fs::write(&script, lines.join("\n")).unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    script
}

pub fn create_test_dir(workspace: &TempDir, filenames: &[&str]) -> PathBuf {
    let dir = workspace.path().join("testdir");
    std::fs::create_dir_all(&dir).unwrap();
    for name in filenames {
        let path = dir.join(name);
        std::fs::write(&path, format!("content of {}", name)).unwrap();
        let epoch = UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(epoch)).unwrap();
    }
    dir
}

// ---------------------------------------------------------------------------
// Direct DB access helpers
// ---------------------------------------------------------------------------

pub fn open_test_db(db_path: &str) -> Connection {
    if let Some(parent) = std::path::Path::new(db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let conn = Connection::open(db_path).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS descriptions (
            fname      TEXT,
            dir        TEXT,
            description TEXT,
            mtime      REAL
        )",
        [],
    ).unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS vectors (
            fname      TEXT,
            dir        TEXT,
            embedding  BLOB,
            mtime      REAL
        )",
        [],
    ).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn
}

#[allow(dead_code)]
pub fn query_descriptions(conn: &Connection) -> Vec<(String, String)> {
    let mut stmt = conn.prepare(
        "SELECT fname, description FROM descriptions ORDER BY fname"
    ).unwrap();
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

pub fn count_descriptions(conn: &Connection) -> usize {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM descriptions").unwrap();
    stmt.query_row([], |row| row.get::<_, usize>(0)).unwrap()
}

pub fn count_vectors(conn: &Connection) -> usize {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM vectors").unwrap();
    stmt.query_row([], |row| row.get::<_, usize>(0)).unwrap()
}

pub fn query_vectors(conn: &Connection) -> Vec<(String, Vec<u8>)> {
    let mut stmt = conn.prepare(
        "SELECT fname, embedding FROM vectors ORDER BY fname"
    ).unwrap();
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

#[allow(dead_code)]
pub fn clear_descriptions(conn: &Connection) {
    conn.execute("DELETE FROM descriptions", []).unwrap();
}

// ---------------------------------------------------------------------------
// Portal guard — runs the debug portal on an isolated D-Bus name/path
// ---------------------------------------------------------------------------

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Returns a unique, collision-free D-Bus service name and object path.
pub fn make_unique_name() -> (String, String) {
    // Use a high-precision timestamp + counter to guarantee uniqueness across
    // test runs (D-Bus holds released names for ~30s).
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    (
        format!("org.freedesktop.impl.portal.desktop.pikeru.t{:04}{}", ts / 1_000_000, id),
        format!("/org/freedesktop/portal/desktop/test/p{:04}", id),
    )
}

pub struct PortalGuard {
    child: Option<std::process::Child>,
    pub service_name: String,
    pub object_path: String,
}

impl PortalGuard {


    pub fn new(db_path: &str, config_path: &str) -> Self {
        let (svc, obj) = make_unique_name();
        let portal_bin = env!("CARGO_BIN_EXE_portal");
        let mut child = std::process::Command::new(portal_bin)
            .args(["-c", config_path, "-d", db_path])
            .args(["-s", &svc]).args(["-p", &obj])
            .spawn().expect("Failed to spawn portal binary");

        // Verify portal is still running before waiting
        if let Ok(Some(code)) = child.try_wait() {
            drop(child);
            panic!("Portal exited immediately with code: {}", code);
        }

        // Wait for the portal to register on D-Bus (poll via python helper)
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/dbus_call.py")
            .to_string_lossy().into_owned();
        let mut waited = 0;
        loop {
            let output = Command::new("python3")
                .args([&script, "_ping", &svc])
                .output();
            if let Ok(out) = output {
                if out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "OK" {
    
                    return Self {
                        child: Some(child),
                        service_name: svc,
                        object_path: obj,
                    };
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            waited += 100;

            if waited > 5000 {
                panic!("Portal failed to register on D-Bus after 5s. svc={}", svc);
            }
        }
    }

    #[allow(dead_code)]
    pub fn dispose(self) {}
}

impl Drop for PortalGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

// ---------------------------------------------------------------------------
// Test client — uses dbus-python via the python helper script
// ---------------------------------------------------------------------------

pub struct OpenResult { pub status: u32, pub uris: Vec<String> }

pub struct PortalClient {
    service_name: String,
    object_path: String,
}

impl PortalClient {
    pub fn new(service_name: &str, object_path: &str) -> Self {
        Self { service_name: service_name.to_string(), object_path: object_path.to_string() }
    }

    pub fn open_file(&self, multiple: bool, directory: bool) -> Result<OpenResult, String> {
        let multi_str = if multiple { "true" } else { "false" };
        let dir_str = if directory { "true" } else { "false" };

        // Wait for portal to be ready (handles race with D-Bus name registration)
        self._wait_for_portal()?;

        let script_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/dbus_call.py").to_string_lossy().into_owned();
        let output = Command::new("python3")
            .args([&script_path, "open_file", &self.service_name, &self.object_path, multi_str, dir_str])
            .output()
            .map_err(|e| format!("python3 failed: {}", e))?;

        if !output.status.success() {
            return Err(format!("dbus call failed: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.starts_with("STATUS:") {
            return Err(format!("unexpected output: {}", stdout));
        }

        let rest = &stdout[7..]; // skip "STATUS:"
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let status: u32 = parts[0].parse::<u32>().map_err(|e| e.to_string())?;

        let uris = if status == 0 && parts.len() > 1 {
            parts[1][5..].to_string() // skip "URIS:"
                .split('|')
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        } else {
            Vec::new()
        };

        Ok(OpenResult { status, uris })
    }

    pub fn configure_indexer(&self, respect_gitignore: bool, search_ignore: &str) -> Result<(), String> {
        self._wait_for_portal()?;
        let gi_str = if respect_gitignore { "true" } else { "false" };
        self._dbus_method("configure", &[&self.service_name, &self.object_path, gi_str, search_ignore])
    }

    pub fn update_index(&self, dirs: &[&str]) -> Result<(), String> {
        self._wait_for_portal()?;
        let mut args: Vec<String> = vec![self.service_name.clone(), self.object_path.clone()];
        for d in dirs { args.push(d.to_string()); }
        self._dbus_method("update", &args.iter().map(|s| s.as_str()).collect::<Vec<&str>>())
    }

    pub fn clear_index_queue(&self) -> Result<(), String> {
        self._wait_for_portal()?;
        self._dbus_method("clear_queue", &[&self.service_name, &self.object_path])
    }

    /// Call the TextEmbed method on the SearchIndexer interface.
    /// Returns the raw binary embedding as Vec<u8>, or an error string.
    pub fn text_embed(&self, text: &str) -> Result<Vec<u8>, String> {
        self._wait_for_portal()?;
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/dbus_call.py").to_string_lossy().into_owned();
        let output = Command::new("python3")
            .args([&script, "text_embed", &self.service_name, &self.object_path, text])
            .output()
            .map_err(|e| format!("python3 failed: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.starts_with("EMBED:") {
            return Err(format!("text_embed unexpected output: {}", stdout));
        }
        let hex_str = &stdout[6..];
        let bytes: Vec<u8> = (0..hex_str.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex_str[i..i+2], 16))
            .collect::<Result<Vec<u8>, _>>()
            .map_err(|e| format!("failed to parse hex embedding: {}", e))?;
        Ok(bytes)
    }

    fn _wait_for_portal(&self) -> Result<(), String> {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/dbus_call.py").to_string_lossy().into_owned();
        for wait_ms in [20, 50, 100, 100, 100, 100, 200, 200, 300] {
            let output = Command::new("python3")
                .args([&script, "_ping", &self.service_name])
                .output()
                .map_err(|e| format!("ping failed: {}", e))?;
            if output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "OK" {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(wait_ms));
        }
        Err(format!("Portal not ready after ~2s: service={}", self.service_name))
    }

    fn _dbus_method(&self, method: &str, args: &[&str]) -> Result<(), String> {
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/dbus_call.py").to_string_lossy().into_owned();
        let mut cmd_args: Vec<String> = vec![script, method.to_string()];
        for a in args { cmd_args.push(a.to_string()); }

        let output = Command::new("python3")
            .args(&cmd_args)
            .output()
            .map_err(|e| format!("python3 failed: {}", e))?;

        if !output.status.success() {
            return Err(format!("{} failed: {}", method, String::from_utf8_lossy(&output.stderr)));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if stdout == "OK" { Ok(()) } else {
            Err(format!("{} returned: {}", method, stdout))
        }
    }
}
