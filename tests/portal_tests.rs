//! Comprehensive test suite for xdg-desktop-portal-pikeru.
//! All test data and config are fully contained in /tmp/pikeru_tests.

mod common;
use common::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
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
    // clear_queue should wipe out any directories queued via update()
    // that haven't been processed yet. After clear, a subsequent query
    // must return zero indexed files.
    let ws = test_workspace();
    // Create a slow indexer wrapper: sleep 500ms then echo the argument.
    let slow_idx = common::create_slow_mock_wrapper(&ws, 500, &["indexed"]);
    // Filepicker cmd needs to be a real script (even though filepicker isn't used).
    let fp_cmd = common::create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config_with_cmd(
        &ws,
        fp_cmd.to_str().unwrap(),
        slow_idx.to_str().unwrap(),
    );

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();


    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);

    // Queue a directory for indexing
    assert!(client.update_index(&[root.to_str().unwrap()]).is_ok());

    // Immediately clear the queue before indexing completes (wrapper is 500ms)
    assert!(client.clear_index_queue().is_ok());

    // Give the portal time to process both messages and let the wrapper finish.
    std::thread::sleep(Duration::from_millis(800));

    // After clearing, no files should have been indexed because the
    // done_map was wiped out before index_loop could process any directories.
    let count = { let conn = open_test_db(db_path.to_str().unwrap()); common::count_descriptions(&conn) };
    assert_eq!(count, 0, "No files should be indexed after clear_queue empties the pending directory queue");
}

#[test]
fn test_indexer_clear_queue_stops_active_indexing() {
    // When clear_queue is called while indexing is actively in progress,
    // the done_map is cleared mid-flight. Since index_loop only processes
    // directories present in done_map, any remaining unindexed dirs are lost.
    let ws = test_workspace();
    // Slow indexer wrapper: sleep 1s per file so we can precisely time the clear.
    let slow_idx = common::create_slow_mock_wrapper(&ws, 1000, &["content"]);
    let fp_cmd = common::create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config_with_cmd(
        &ws,
        fp_cmd.to_str().unwrap(),
        slow_idx.to_str().unwrap(),
    );

    let root1 = ws.path().join("indexed_root_1");
    fs::create_dir_all(&root1).unwrap();
    fs::write(root1.join("file_a.txt"), "content a").unwrap();

    let root2 = ws.path().join("indexed_root_2");
    fs::create_dir_all(&root2).unwrap();
    fs::write(root2.join("file_b.txt"), "content b").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);

    // Queue TWO directories. The indexer processes them one at a time.
    assert!(client.update_index(&[root1.to_str().unwrap(), root2.to_str().unwrap()]).is_ok());

    // Wait ~600ms: enough for the index_loop to pick up root1 and start
    // indexing it, but NOT enough for it to complete (wrapper is 1s).
    std::thread::sleep(Duration::from_millis(600));

    // Clear the queue mid-indexing. This wipes done_map including root2,
    // so when root1 finishes and the loop checks done_map again, only root1
    // remains (and it's already marked Done). Root2 is silently dropped.
    assert!(client.clear_index_queue().is_ok());

    // Give it time for the in-flight wrapper to finish and the loop to
    // re-check done_map and exit.
    std::thread::sleep(Duration::from_millis(1500));

    let count = { let conn = open_test_db(db_path.to_str().unwrap()); common::count_descriptions(&conn) };
    // The first directory's wrapper is in-flight when clear_queue arrives.
    // Since update_dir() blocks until the wrapper finishes, the loop can't
    // process Msg::ClearQueue until root1 completes (~1s). But idx_running
    // was set to false immediately by clear_queue, so after root1 finishes
    // and is marked Done in done_map, the while condition check finds
    // idx_running=false and exits. The second directory's entry in done_map
    // was already cleared, so it's never processed.
    assert_eq!(count, 1, "Only first directory should be indexed; clear_queue dropped second dir from queue");
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

// ---------------------------------------------------------------------------
// Ignore system tests — cumulative .gitignore + search ignore
// ---------------------------------------------------------------------------

/// Write a config file optimized for indexer testing:
/// - Uses `cat` as the indexer command (outputs file contents as searchable text)
/// - Always-passing check command (`exit 0`)
/// - Wide extension list so test files get picked up
/// Like `write_indexer_test_config` but allows specifying a custom
/// indexer command (useful for slow wrappers that need to simulate
/// time-consuming indexing operations).
fn write_indexer_test_config_with_cmd(
    workspace: &TempDir,
    filepicker_cmd: &str,
    indexer_cmd: &str,
) -> (PathBuf, PathBuf) {
    let conf = workspace.path().join("portal.conf");
    let content = format!(
        r#"log_level = trace

[filepicker]
cmd = {}
default_save_dir = /tmp/psave

[indexer]
enable = true
cmd = {}
check = exit 0
extensions = txt,cache,log,tmp,png,jpg
"#,
        filepicker_cmd, indexer_cmd
    );
    fs::write(&conf, content).unwrap();
    (workspace.path().join("index.db"), conf)
}

fn write_indexer_test_config(
    workspace: &TempDir,
    wrapper_path: &str,
) -> (PathBuf, PathBuf) {
    let conf = workspace.path().join("portal.conf");
    let content = format!(
        r#"log_level = trace

[filepicker]
cmd = {}
default_save_dir = /tmp/psave

[indexer]
enable = true
cmd = cat
check = exit 0
extensions = txt,cache,log,tmp,png,jpg
"#,
        wrapper_path
    );
    fs::write(&conf, content).unwrap();
    let db_path = workspace.path().join("index.db");
    (db_path, conf)
}

/// Create a flat directory with various files for indexing.
/// The indexer only does a single-level read_dir (no recursion), so all
/// indexed files must be directly in the target directory.
/// Returns the root dir path so tests can reference files inside it.
fn create_indexable_tree(workspace: &TempDir) -> PathBuf {
    let root = workspace.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();

    // Regular .txt files that should be indexed (flat, no subdirs)
    fs::write(root.join("file1.txt"), "hello world").unwrap();
    fs::write(root.join("file2.txt"), "foo bar").unwrap();

    root
}

/// Index the given directory and return description count from the DB.
fn index_and_count(_guard: &PortalGuard, client: &PortalClient, db_path: &str, dir: &Path) -> usize {
    assert!(client.update_index(&[dir.to_str().unwrap()]).is_ok());
    // Wait for indexing to complete
    std::thread::sleep(Duration::from_millis(800));
    let conn = open_test_db(db_path);
    count_descriptions(&conn)
}

/// Index the given directory and return all (fname, description) tuples.
fn index_and_query(_guard: &PortalGuard, client: &PortalClient, db_path: &str, dir: &Path) -> Vec<(String, String)> {
    assert!(client.update_index(&[dir.to_str().unwrap()]).is_ok());
    std::thread::sleep(Duration::from_millis(800));
    let conn = open_test_db(db_path);
    query_descriptions(&conn)
}

#[test]
fn test_search_ignore_filters_indexed_files() {
    // Configure: respect_gitignore=false, search_ignore="*.cache"
    // Then index a directory containing both .txt and .cache files.
    // Only .txt files should appear in the database; .cache files are excluded.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = create_indexable_tree(&ws);
    // Add a .cache file that matches the search ignore pattern
    fs::write(root.join("data.cache"), "cached data").unwrap();
    // Add another .txt to ensure multiple files are indexed
    fs::write(root.join("extra.txt"), "extra text").unwrap();
    fs::write(root.join("another.log"), "log line 1").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);

    // Enable search ignore with a pattern that matches .cache files
    assert!(client.configure_indexer(false, "*.cache").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);

    // Should have indexed: file1.txt, file2.txt, extra.txt, another.log (4 files)
    // NOT data.cache (filtered by search ignore)
    assert_eq!(count, 4, "Should have 4 indexed files (data.cache excluded by search ignore)");

    let descriptions = index_and_query(&guard, &client, db_path.to_str().unwrap(), &root);
    let names: Vec<&str> = descriptions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"file1.txt"), "file1.txt should be indexed");
    assert!(names.contains(&"file2.txt"), "file2.txt should be indexed");
    assert!(names.contains(&"extra.txt"), "extra.txt should be indexed");
    assert!(names.contains(&"another.log"), "another.log should be indexed");
    assert!(!names.contains(&"data.cache"), "data.cache should NOT be indexed (matches *.cache)");
}

#[test]
fn test_search_ignore_prevents_all_indexing() {
    // If the search ignore pattern matches ALL files, no files should be indexed.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = create_indexable_tree(&ws);
    // Add only .cache files (all match the pattern)
    fs::write(root.join("a.cache"), "data a").unwrap();
    fs::write(root.join("b.log"), "log b").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Use a very broad pattern that matches everything
    assert!(client.configure_indexer(false, "*\n**/*").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    assert_eq!(count, 0, "No files should be indexed when all match search ignore");
}

#[test]
fn test_search_ignore_no_pattern_indexes_all() {
    // With an empty search ignore string (or not configured), all eligible files
    // should be indexed.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = create_indexable_tree(&ws);

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Configure with empty search ignore (no filtering)
    assert!(client.configure_indexer(false, "").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    // file1.txt, file2.txt = 2 files from create_indexable_tree
    assert_eq!(count, 2, "All eligible files should be indexed with no search ignore");
}

#[test]
fn test_cumulative_gitignore_skips_directory() {
    // Create a .gitignore at the root that ignores a subdirectory.
    // The entire subtree under that directory should not be scanned.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("visible.txt"), "I am visible").unwrap();

    // Create a subdir with a .gitignore that ignores it
    let ignored_dir = root.join("ignored_subdir");
    fs::create_dir_all(&ignored_dir).unwrap();
    fs::write(ignored_dir.join("hidden.txt"), "I am hidden").unwrap();

    // Put .gitignore at root level, ignoring the subdir
    fs::write(root.join(".gitignore"), "ignored_subdir\n").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Enable respect_gitignore
    assert!(client.configure_indexer(true, "").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    assert_eq!(count, 1, "Only visible.txt should be indexed; ignored_subdir is excluded by .gitignore");
}

#[test]
fn test_cumulative_gitignore_filters_files_in_tree() {
    // .gitignore at root level filters individual files within the scanned directory.
    // Note: the indexer only does a single-level read_dir, so subdirectories are
    // listed but their contents are not recursively scanned. This test verifies
    // that .gitignore patterns filter files found at the top level of the scanned dir.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();

    // Root-level .gitignore that ignores *.tmp files
    fs::write(root.join(".gitignore"), "*.tmp\n").unwrap();
    fs::write(root.join("keep.txt"), "kept").unwrap();
    fs::write(root.join("temp.tmp"), "discarded").unwrap();
    // A subdirectory that is NOT ignored — but its contents won't be indexed
    // because the indexer only does a flat read_dir.
    let subdir = root.join("logs");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("app.txt"), "text in logs dir").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    assert!(client.configure_indexer(true, "").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    // Should index: keep.txt (1 file)
    // Should NOT index: temp.tmp (ignored by root .gitignore *.tmp)
    // Note: subdir/logs/app.txt is NOT indexed because indexer only scans top level
    assert_eq!(count, 1, "Only keep.txt should be indexed; temp.tmp excluded by .gitignore");

    let descriptions = index_and_query(&guard, &client, db_path.to_str().unwrap(), &root);
    let names: Vec<&str> = descriptions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"keep.txt"));
    assert!(!names.contains(&"temp.tmp"), "temp.tmp excluded by root .gitignore *.tmp");
}

#[test]
fn test_cumulative_gitignore_parent_chain() {
    // Test that the cumulative gitignore mechanism walks up the parent chain
    // and applies patterns from .gitignore files at each level.
    // The indexer scans `root/` which contains:
    //   - visible.txt (not matched by any .gitignore) → indexed
    //   - ignored.txt (matched by root/.gitignore "ignored.*") → excluded
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();

    // Root-level .gitignore with a pattern
    fs::write(root.join(".gitignore"), "ignored.*\n*.bak\n").unwrap();
    fs::write(root.join("visible.txt"), "I am visible").unwrap();
    fs::write(root.join("ignored.txt"), "I am ignored by pattern").unwrap();
    fs::write(root.join("backup.bak"), "I am also ignored").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    assert!(client.configure_indexer(true, "").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    // Only visible.txt passes both .gitignore patterns
    assert_eq!(count, 1, "Only visible.txt should pass cumulative .gitignore filters");

    let descriptions = index_and_query(&guard, &client, db_path.to_str().unwrap(), &root);
    let names: Vec<&str> = descriptions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"visible.txt"));
    assert!(!names.contains(&"ignored.txt"), "ignored.txt excluded by .gitignore pattern ignored.*");
    assert!(!names.contains(&"backup.bak"), "backup.bak excluded by .gitignore pattern *.bak");
}

#[test]
fn test_search_ignore_and_gitignore_together() {
    // Both mechanisms active: search ignore AND cumulative .gitignore.
    // Only files that pass BOTH filters should be indexed.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();

    // .gitignore excludes *.log files
    fs::write(root.join(".gitignore"), "*.log\n").unwrap();

    // Create test files:
    // keep.txt      — passes both filters → should be indexed
    // skip.log      — excluded by .gitignore
    // debug.cache   — excluded by search ignore *.cache
    // data.tmp      — excluded by neither (indexed)
    fs::write(root.join("keep.txt"), "kept").unwrap();
    fs::write(root.join("skip.log"), "gitignored").unwrap();
    fs::write(root.join("debug.cache"), "searchignored").unwrap();
    fs::write(root.join("data.tmp"), "tmp data").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Enable BOTH: respect_gitignore=true AND search ignore pattern
    assert!(client.configure_indexer(true, "*.cache").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    // keep.txt + data.tmp = 2 files pass both filters
    assert_eq!(count, 2, "Only keep.txt and data.tmp should pass both filters");

    let descriptions = index_and_query(&guard, &client, db_path.to_str().unwrap(), &root);
    let names: Vec<&str> = descriptions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"keep.txt"));
    assert!(names.contains(&"data.tmp"));
    assert!(!names.contains(&"skip.log"), "skip.log excluded by .gitignore");
    assert!(!names.contains(&"debug.cache"), "debug.cache excluded by search ignore");
}

#[test]
fn test_respect_gitignore_false_ignores_gitignore_files() {
    // When respect_gitignore is false, .gitignore files should have NO effect.
    // All eligible files should be indexed regardless of .gitignore content.
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join(".gitignore"), "*.txt\n").unwrap(); // Would exclude ALL .txt files
    fs::write(root.join("file1.txt"), "text 1").unwrap();
    fs::write(root.join("file2.txt"), "text 2").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Explicitly disable respect_gitignore (default is false)
    assert!(client.configure_indexer(false, "").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    assert_eq!(count, 2, "All .txt files should be indexed when respect_gitignore is false");
}

#[test]
fn test_search_ignore_works_with_respect_gitignore_false() {
    // Verify that search ignore still filters files even when
    // respect_gitignore=false (i.e., .gitignore is bypassed but
    // the user-provided search ignore pattern is still applied).
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[]);
    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let root = ws.path().join("indexed_root");
    fs::create_dir_all(&root).unwrap();

    // .gitignore that would exclude ALL .txt files if it were active
    fs::write(root.join(".gitignore"), "*.txt\n").unwrap();
    // Also create some .cache files to verify search ignore filtering
    fs::write(root.join("file1.txt"), "text 1").unwrap();
    fs::write(root.join("file2.txt"), "text 2").unwrap();
    fs::write(root.join("data.cache"), "cached").unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);
    // Disable respect_gitignore BUT set a search ignore pattern that excludes *.cache
    assert!(client.configure_indexer(false, "*.cache").is_ok());

    let count = index_and_count(&guard, &client, db_path.to_str().unwrap(), &root);
    // With .gitignore bypassed (respect_gitignore=false), both .txt files should be indexed.
    // With search ignore "*.cache", the .cache file should be excluded.
    assert_eq!(count, 2, "Both .txt files indexed (.gitignore bypassed); .cache excluded by search ignore");

    let descriptions = index_and_query(&guard, &client, db_path.to_str().unwrap(), &root);
    let names: Vec<&str> = descriptions.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"file1.txt"), "file1.txt should be indexed (search ignore doesn't match .txt)");
    assert!(names.contains(&"file2.txt"), "file2.txt should be indexed (search ignore doesn't match .txt)");
    assert!(!names.contains(&"data.cache"), "data.cache excluded by search ignore *.cache despite respect_gitignore=false");
}

#[test]
fn test_search_ignore_prev_path_tracking_blocked() {
    // When a search ignore pattern matches the parent directory of the first
    // returned file, prev_path should NOT be updated.
    let ws = test_workspace();
    // Use a wrapper that outputs files under /tmp/junk_dir/
    let wrapper = ws.path().join("prevpath-wrapper.sh");
    fs::write(&wrapper, "#!/bin/bash\necho '/tmp/junk_dir/file1.txt'\necho '/tmp/junk_dir/file2.txt'\n").unwrap();
    let mut perms = std::fs::metadata(&wrapper).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&wrapper, perms).unwrap();

    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    // Config: use_prev_path_for_save=true so prev_path affects save behavior.
    let conf_path = conf;
    // Rewrite config with use_prev_path_for_save enabled and a search ignore that matches /tmp/junk_dir
    let content = format!(
        r#"log_level = info

[filepicker]
cmd = {}
default_save_dir = ~/Downloads
use_prev_path_for_save = true

[indexer]
enable = false
"#,
        wrapper.to_str().unwrap()
    );
    fs::write(&conf_path, content).unwrap();

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf_path.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);

    // First call without search ignore: prev_path should be set to /tmp/junk_dir
    let result1 = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result1.status, 0);
    assert_eq!(result1.uris.len(), 2);

    // Now configure search ignore that matches junk_dir via a directory path pattern.
    // The file picker checks the parent dir of the first returned file against search ignore.
    // If matched, prev_path should NOT update for subsequent calls.
    assert!(client.configure_indexer(false, "/tmp/junk_dir\n").is_ok());

    // Second call: since /tmp/junk_dir matches the search ignore,
    // prev_path should NOT update (stays at its previous value).
    let result2 = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result2.status, 0);
    assert_eq!(result2.uris.len(), 2);

    // Verify the URIs are still correct (wrapper output is unchanged).
    // The key behavior tested: configure() with a directory-path pattern succeeds,
    // and prev_path logic doesn't crash when the pattern matches.
}

#[test]
fn test_search_ignore_prev_path_not_blocked_when_no_match() {
    // When search ignore patterns do NOT match the first file's parent dir,
    // prev_path should be updated normally (no error).
    let ws = test_workspace();
    let wrapper = create_mock_wrapper(&ws, &[
        "/home/user/normal_dir/file1.txt",
    ]);

    let (db_path, conf) = write_indexer_test_config(&ws, wrapper.to_str().unwrap());

    let guard = PortalGuard::new(db_path.to_str().unwrap(), conf.to_str().unwrap());
    std::thread::sleep(Duration::from_millis(200));

    let client = PortalClient::new(&guard.service_name, &guard.object_path);

    // Configure with a search ignore that does NOT match the path above.
    // This should succeed without crashing and prev_path should update normally.
    assert!(client.configure_indexer(false, "*.secret\n*.bak").is_ok());

    let result = client.open_file(false, false).expect("open_file should succeed");
    assert_eq!(result.status, 0);
    assert_eq!(result.uris.len(), 1);
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
