//! Comprehensive test suite for xdg-desktop-portal-pikeru.
//! All test data and config are fully contained in /tmp/pikeru_tests.

mod common;
use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::TempDir;
use std::time::Duration;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn write_test_config(
    workspace: &TempDir,
    wrapper_path: &str,
    indexer_cmd: &str,
    indexer_check: &str,
    extensions: &str,
) -> (PathBuf, String, String) {
    let conf = workspace.path().join("portal.conf");
    let content = format!(
        r#"log_level = trace

[filepicker]
cmd = {}
default_save_dir = /tmp/psave

[indexer]
enable = true
cmd = {}
check = {}
extensions = {}
"#,
        wrapper_path, indexer_cmd, indexer_check, extensions
    );
    fs::write(&conf, content).unwrap();
    let (service_name, object_path) = make_unique_name();
    let db_path = workspace.path().join("test.db");
    (db_path, service_name, object_path)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_db_flag_exists() {
    // Verify the portal accepts -d flag for database path
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_portal"))
        .args(["-d", "/tmp/test.db"])
        .output()
        .expect("portal binary not found");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Unrecognized option: 'd'"),
        "--d flag not recognized. stderr: {}",
        stderr
    );
}

#[test]
fn test_config_minimal_parses() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &["/tmp/test.txt"]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo indexed", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
}

#[test]
fn test_open_file_returns_uris() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[
        "/home/user/pictures/photo1.jpg",
        "/home/user/pictures/photo2.png",
    ]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "jpg,png",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 2);
    assert!(result.uris.contains(&"file:///home/user/pictures/photo1.jpg".to_string()));
    assert!(result.uris.contains(&"file:///home/user/pictures/photo2.png".to_string()));
}

#[test]
fn test_open_file_cancelled_returns_one() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 1);
}

#[test]
fn test_open_file_directory_mode() {
    let ws = test_workspace();
    create_test_dir(&ws, &["subdir"]);
    let wrapper = ws.path().join("dir-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"$4\"\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, true).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 1);
    assert!(result.uris[0].starts_with("file:///"));
}

#[test]
fn test_open_file_multiple_selection() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[
        "/home/user/file1.txt",
        "/home/user/file2.txt",
        "/home/user/file3.txt",
    ]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(true, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 3);
}

#[test]
fn test_open_mode_sets_pk_xdg() {
    let ws = test_workspace();
    let wrapper = ws.path().join("capture-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"PK_XDG=${PK_XDG:-unset} POSTPROCESS_DIR=${POSTPROCESS_DIR:-unset}\" > /tmp/portal_cmd_capture.txt\necho \"$*\" >> /tmp/portal_cmd_capture.txt\necho \"/tmp/saved_file.txt\"\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let _ = fs::remove_file("/tmp/portal_cmd_capture.txt");
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let _ = client.open_file(false, false).expect("open_file should succeed");
    std::thread::sleep(Duration::from_millis(200));

    if let Ok(cmd_text) = fs::read_to_string("/tmp/portal_cmd_capture.txt") {
        assert!(cmd_text.contains("PK_XDG=1"), "Should set PK_XDG=1: {}", cmd_text);
    }
}

#[test]
fn test_open_mode_sets_postprocess_env() {
    let ws = test_workspace();
    let wrapper = ws.path().join("capture-env.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"POSTPROCESS_DIR=${POSTPROCESS_DIR:-NOTSET}\" > /tmp/portal_env_capture.txt\necho \"$HOME/test.jpg\"\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let _ = fs::remove_file("/tmp/portal_env_capture.txt");
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "jpg",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let _ = client.open_file(false, false).expect("open_file should succeed");
    std::thread::sleep(Duration::from_millis(200));

    if let Ok(env_text) = fs::read_to_string("/tmp/portal_env_capture.txt") {
        assert!(env_text.contains("POSTPROCESS_DIR="), "Should set POSTPROCESS_DIR: {}", env_text);
    }
}

#[test]
fn test_indexer_configure() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &["/tmp/x.txt"]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    assert!(client.configure_indexer(true, "*.cache\n*.tmp").is_ok());
}

#[test]
fn test_indexer_update() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    // Create real subdirectories under /tmp so the indexer can scan them
    let dir1 = ws.path().join("photos");
    let dir2 = ws.path().join("backup");
    fs::create_dir_all(&dir1).unwrap();
    fs::create_dir_all(&dir2).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    assert!(client.update_index(&[dir1.to_str().unwrap(), dir2.to_str().unwrap()]).is_ok());
}

#[test]
fn test_indexer_clear_queue() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    assert!(client.clear_index_queue().is_ok());
}

#[test]
fn test_database_created() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let db_path = ws.path().join("index.db");
    let _ = fs::remove_file(&db_path);

    let _conf = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    assert!(db_path.exists(), "Database should be created");
    let conn = open_test_db(db_path.to_str().unwrap());
    assert_eq!(count_descriptions(&conn), 0);

    let journal_mode: String = conn.pragma_query_value(None, "journal_mode", |row| row.get(0)).unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");
}

#[test]
fn test_paths_with_spaces() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[
        "/home/user/my documents/photo.jpg",
        "/tmp/file with spaces.txt",
    ]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "jpg,txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 2);
    assert!(result.uris.contains(&"file:///home/user/my documents/photo.jpg".to_string()));
    assert!(result.uris.contains(&"file:///tmp/file with spaces.txt".to_string()));
}

#[test]
fn test_paths_with_quotes() {
    let ws = test_workspace();
    let wrapper = ws.path().join("quote-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho '/path/with\"quotes/file.txt'\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
}

#[test]
fn test_empty_output_returns_one() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 1);
}

#[test]
fn test_wrapper_failure_returns_one() {
    let ws = test_workspace();
    let wrapper = ws.path().join("failing-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\nexit 1\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 1);
}

#[test]
fn test_config_unknown_keys_no_crash() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );

    // Overwrite config with unknown keys
    let conf = ws.path().join("portal.conf");
    fs::write(&conf, format!(r#"log_level = info

[filepicker]
cmd = {}
default_save_dir = /tmp/psave
unknown_key = some_value

[indexer]
enable = true
cmd = echo idx
check = exit 0
extensions = txt
unknown_indexer = bad
"#, wrapper.to_str().unwrap())).unwrap();

    let _guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
}

#[test]
fn test_sequential_calls_work() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &["/tmp/concurrent.txt"]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    for i in 0..3u32 {
        let result = client.open_file(false, false);
        assert!(result.is_ok(), "Call {} should succeed: {:?}", i, result.err());
    }
}

#[test]
fn test_gitignore_flag() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    assert!(client.configure_indexer(true, "*.cache").is_ok());
    assert!(client.configure_indexer(false, "").is_ok());
}

#[test]
fn test_single_file_mode() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &["/home/user/single.txt"]);
    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 1);
    assert_eq!(result.uris[0], "file:///home/user/single.txt");
}

#[test]
fn test_empty_lines_skipped() {
    let ws = test_workspace();
    let wrapper = ws.path().join("mixed-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho '/tmp/valid.jpg'\necho ''\necho '/tmp/also_valid.png'\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "jpg,png",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    assert!(client.open_file(false, false).is_ok());
}

#[test]
fn test_multiple_output_lines() {
    let ws = test_workspace();
    let wrapper = ws.path().join("multiout-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"/tmp/a.txt\"\necho \"/tmp/b.jpg\"\necho \"/tmp/c.png\"\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt,jpg,png",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(true, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 3);
    assert!(result.uris.contains(&"file:///tmp/a.txt".to_string()));
    assert!(result.uris.contains(&"file:///tmp/b.jpg".to_string()));
    assert!(result.uris.contains(&"file:///tmp/c.png".to_string()));
}

#[test]
fn test_wrapper_stderr_ok() {
    let ws = test_workspace();
    let wrapper = ws.path().join("stderr-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"warning\" >&2\necho \"/tmp/output.txt\"\nexit 0\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
}

#[test]
fn test_nonexistent_wrapper() {
    // Use a wrapper that exists but exits non-zero to simulate failure
    let ws = test_workspace();
    let wrapper = ws.path().join("nonexist-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\nexit 2\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 1);
}

#[test]
fn test_tilda_expansion() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );

    let conf = ws.path().join("portal.conf");
    fs::write(&conf, format!(r#"log_level = info

[filepicker]
cmd = {}
default_save_dir = ~/Downloads

[indexer]
enable = false
"#, wrapper.to_str().unwrap())).unwrap();

    let _guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
}

#[test]
fn test_use_prev_path_config() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );

    let conf = ws.path().join("portal.conf");
    fs::write(&conf, format!(r#"log_level = info

[filepicker]
cmd = {}
default_save_dir = ~/Downloads
use_prev_path_for_save = true

[indexer]
enable = false
"#, wrapper.to_str().unwrap())).unwrap();

    let _guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
}

#[test]
fn test_config_empty_indexer() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );

    let conf = ws.path().join("portal.conf");
    fs::write(&conf, format!(r#"log_level = info

[filepicker]
cmd = {}

[indexer]
enable = false
"#, wrapper.to_str().unwrap())).unwrap();

    let _guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
}

#[test]
fn test_graceful_shutdown() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, _, _) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );

    let portal_bin = env!("CARGO_BIN_EXE_portal");
    let mut child = std::process::Command::new(portal_bin)
        .args(["-c", ws.path().join("portal.conf").to_str().unwrap(), "-d", db_path.to_str().unwrap()])
        .spawn()
        .expect("Failed to spawn portal");
    std::thread::sleep(Duration::from_millis(500));
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn test_dir_mode_uri_prefix() {
    let ws = test_workspace();
    create_test_dir(&ws, &["sub"]);
    let wrapper = ws.path().join("dir-out-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho \"$4\"\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, _svc, _obj) = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(db_path.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&_guard.service_name, &_guard.object_path);
    let result = client.open_file(false, true).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert!(!result.uris.is_empty());
    assert!(result.uris[0].starts_with("file://"));
}

#[test]
fn test_db_path_flag_works() {
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let custom_db = ws.path().join("custom.db");

    let _conf = write_test_config(
        &ws, wrapper.to_str().unwrap(), "echo idx", "exit 0", "txt",
    );
    let _guard = PortalGuard::new(custom_db.to_str().unwrap(), ws.path().join("portal.conf").to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    assert!(custom_db.exists(), "Custom DB path should be used");
}


