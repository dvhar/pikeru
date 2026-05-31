//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use getopts::Options;
use zbus::{
    connection, interface,
    zvariant::{Value,OwnedValue,ObjectPath,
    to_bytes,LE,serialized::Context
    }
};
use std::{
    fs,
    error::Error, future::pending,
    collections::HashMap,
    borrow::Cow,
    path::Path,
    mem::take,
    sync::Arc,
};
use std::time::SystemTime;
use rusqlite;
use std::cmp::Ordering;
use tokio::{
    sync::Mutex as AsyncMtx,
    time::{sleep, Duration, Instant},
};extern crate chrono;
mod logger;
use crate::logger::{LevelFilter, Builder};
use ctrlc;
use ignore::{gitignore,Match};


#[derive(Default, Debug)]
struct Shtate {
    /// Shared between Indexer and FilePicker — the search-ignore patterns
    /// entered in the file picker UI.
    current_searchignore: String,
}

fn build_cumulative_gitignore(start_dir: &Path) -> gitignore::Gitignore {
    let mut dirs = Vec::new();
    let mut cur = start_dir;
    loop {
        dirs.push(cur.to_path_buf());
        if let Some(parent) = cur.parent() {
            cur = parent;
        } else {
            break;
        }
    }
    dirs.reverse();
    let mut builder = gitignore::GitignoreBuilder::new("");
    for dir in dirs {
        let gi_path = dir.join(".gitignore");
        if gi_path.is_file() {
            trace!("Building with gitignore: {:?}\n{}", gi_path, std::fs::read_to_string(&gi_path).unwrap());
            builder.add(gi_path);
        }
    }
    builder.build().unwrap_or(gitignore::Gitignore::new("").0)
}

/// Shared state behind an Arc<RwLock<>>, so clones are just handles to the same data.
struct IndexerInner {
    shtate: Arc<AsyncMtx<Shtate>>,
    con: Arc<std::sync::Mutex<rusqlite::Connection>>,
    done_map: HashMap<String,bool>,
    cmd: String,
    check: String,
    exts: Vec<&'static str>,
    search_ignore: gitignore::Gitignore,
    igtxt: String,
    idx_running: bool,
    indexer_enabled: bool,
    indexer_mode: IndexerMode,
    respect_gitignore: bool,
}

/// Thin handle wrapper — like FItem(Box<FItemb>) pattern.
/// Cloning Indexer clones the Arc handle; all clones point to the same underlying state.
#[derive(Clone)]
struct Indexer(Arc<tokio::sync::RwLock<IndexerInner>>);

impl std::ops::Deref for Indexer {
    type Target = tokio::sync::RwLock<IndexerInner>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[interface(name = "org.freedesktop.impl.portal.SearchIndexer")]
impl Indexer {
    async fn clear_queue(&self) {
        debug!("Got clear queue message");
        let mut inner = self.write().await;
        if !inner.indexer_enabled { return; }
        inner.idx_running = false;
        inner.done_map.clear();
        info!("Cleared indexing queue");
    }
    async fn update(&self, dirs: Vec<String>) {
        // Gate on indexer_enabled — disabled indexers silently ignore update requests.
        let enabled = self.read().await.indexer_enabled;
        if !enabled || dirs.is_empty() { return; }
        // Check + set idx_running atomically so we never spawn duplicate loops.
        let should_spawn;
        {
            let mut inner = self.write().await;
            should_spawn = !inner.idx_running;
            inner.idx_running = true;
            if should_spawn {
                // Fresh loop — start with a clean done_map populated by the new dirs.
                inner.done_map.clear();
            }
            for dir in dirs { inner.done_map.entry(dir).or_default(); }
        }
        // Spawn a new loop only if one wasn't already running.
        if should_spawn {
            let this = self.clone();
            tokio::spawn(async move {
                this.index_loop().await;
            });
        }
    }
    async fn configure(&self, respect_gitignore: bool, search_ignore: String) {
        trace!("Got gitignore configure request: {}", search_ignore);
        // Update indexer-local state in a scoped block so the lock is released
        // before we call update_ignore() (which also needs to lock self).
        {
            let mut inner = self.write().await;
            inner.respect_gitignore = respect_gitignore;
            inner.shtate.lock().await.current_searchignore = search_ignore;
        }
        // Rebuild the ignore matcher.
        self.update_ignore().await;
    }

}

impl Indexer {

    async fn index_loop(self: &Self) {
        // Start with a 1-minute timeout so we check indexer online status right away.
        let mut timeout = Instant::now().checked_add(Duration::from_secs(60)).unwrap();
        if !self.read().await.idx_running {
            warn!("index_loop: idx_running is false, nothing to index");
            return;
        }
        loop {
            // Check indexer online status periodically.
            let uptodate = timeout.cmp(&Instant::now()) == Ordering::Greater;
            if !uptodate {
                timeout = timeout.checked_add(Duration::from_secs(60)).unwrap();
                let online = self.indexer_online().await;
                if !online { warn!("indexer offline"); }
            }
            if !self.read().await.idx_running {
                debug!("index_loop: idx_running cleared, exiting");
                break;
            }
            // Pick the single next unprocessed dir, process it, mark it done, then loop back.
            let maybe_dir: Option<String> = self.read().await.done_map.iter()
                .find(|(_, done)| !**done)
                .map(|(dir, _)| dir.clone());
            if let Some(dir) = maybe_dir {
                let result = self.update_dir(&dir).await;
                if result == DirResult::Fail {
                    error!("Indexing batch failed");
                    let mut inner = self.write().await;
                    inner.done_map.clear();
                    inner.idx_running = false;
                    break;
                }
                // Mark this dir as done.
                self.write().await.done_map.entry(dir).and_modify(|v| *v = true);
            } else {
                debug!("Indexing batch finished");
                let mut inner = self.write().await;
                inner.done_map.clear();
                inner.idx_running = false;
                break;
            }
        }
    }

    fn new(shtate: Arc<AsyncMtx<Shtate>>, config: &mut Config, con: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        { let c = con.lock().unwrap();
            match c.execute("create table if not exists descriptions
                          (fname text, dir text, description text, mtime real);", ()) {
             Ok(_) => {},
             Err(e) => eprintln!("{}", e),
            };
            match c.execute("create table if not exists vectors
                          (fname text, dir text, embedding blob, mtime real);", ()) {
             Ok(_) => {},
             Err(e) => eprintln!("{}", e),
            };
            let _ = c.pragma_update(None, "journal_mode", "WAL");
        }
        let con2 = con.clone();
        ctrlc::set_handler(move || {
            if let Err(e) = con2.lock().unwrap().cache_flush() {
                eprintln!("failed to flush index db:{}", e);
            }
            eprintln!("Portal closing");
            std::process::exit(0);
        }).expect("Error setting Ctrl-C handler");
        Self(Arc::new(tokio::sync::RwLock::new(IndexerInner {
            shtate,
            con,
            done_map: HashMap::new(),
            cmd: take(&mut config.indexer_cmd),
            check: take(&mut config.indexer_check),
            exts: Box::new(take(&mut config.indexer_exts)).leak().split(',').collect(),
            search_ignore: gitignore::Gitignore::new("").0,
            igtxt: String::new(),
            idx_running: false,
            indexer_enabled: config.indexer_enabled,
            indexer_mode: config.indexer_mode.clone(),
            respect_gitignore: true,
        })))
    }

    async fn update_ignore(self: &Self) {
        let inner = self.read().await;
        let txt = inner.shtate.lock().await.current_searchignore.clone();
        if txt == inner.igtxt {
            return
        }
        drop(inner);
        let mut builder = gitignore::GitignoreBuilder::new("");
        txt.lines().for_each(|line|{builder.add_line(None, line).unwrap();});
        let ignore = match builder.build() {
            Ok(gi) => gi,
            Err(e) => {
                warn!("Bad gitignore: {}: {}", e, txt);
                gitignore::Gitignore::new("").0
            },
        };
        let mut inner = self.write().await;
        inner.igtxt = txt;
        inner.search_ignore = ignore;
    }

    async fn indexer_online(&self) -> bool {
        match tokio::process::Command::new("sh").arg("-c").arg(&self.read().await.check).output().await {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    async fn already_done_description(self: &Self, dir: &String, fname: &str, mtime: f32) -> Entry {
        let guard = self.read().await;
        let c = guard.con.lock().unwrap();
        let mut query = c.prepare("select mtime from descriptions where dir = ?1 and fname = ?2").unwrap();
        let ret = match query.query([dir.as_str(), fname.as_ref()]).unwrap().next() {
            Ok(q) => match q {
                Some(r) => {
                    let prev_time: f32 = r.get(0).unwrap();
                    match prev_time == mtime {
                        true => Entry::Done,
                        false => Entry::Old,
                    }
                },
                None => Entry::None,
            },
            Err(e) => {
                error!("sqlite error: {}", e);
                Entry::Done
            }
        };
        ret
    }

    async fn already_done_vector(self: &Self, dir: &String, fname: &str, mtime: f32) -> Entry {
        let guard = self.read().await;
        let c = guard.con.lock().unwrap();
        let mut query = c.prepare("select mtime from vectors where dir = ?1 and fname = ?2").unwrap();
        let ret = match query.query([dir.as_str(), fname.as_ref()]).unwrap().next() {
            Ok(q) => match q {
                Some(r) => {
                    let prev_time: f32 = r.get(0).unwrap();
                    match prev_time == mtime {
                        true => Entry::Done,
                        false => Entry::Old,
                    }
                },
                None => Entry::None,
            },
            Err(e) => {
                error!("sqlite error: {}", e);
                Entry::Done
            }
        };
        ret
    }

    async fn save_description(self: &Self, dir: &String, fname: &str, desc: &str, mtime: f32, stat: Entry) {
        let guard = self.write().await;
        let c = guard.con.lock().unwrap();
        let mut query = c.prepare(match stat {
            Entry::None => "insert into descriptions (dir, fname, description, mtime) values (?1, ?2, ?3, ?4)",
            Entry::Old => "update descriptions set description = ?3, mtime = ?4 where dir = ?1 and fname = ?2",
            Entry::Done => unreachable!(),
        }).unwrap();
        query.execute((dir, fname, desc, mtime)).unwrap();
    }

    async fn save_vector(self: &Self, dir: &String, fname: &str, embedding: &[u8], mtime: f32, stat: Entry) {
        let guard = self.write().await;
        let c = guard.con.lock().unwrap();
        let mut query = c.prepare(match stat {
            Entry::None => "insert into vectors (dir, fname, embedding, mtime) values (?1, ?2, ?3, ?4)",
            Entry::Old => "update vectors set embedding = ?3, mtime = ?4 where dir = ?1 and fname = ?2",
            Entry::Done => unreachable!(),
        }).unwrap();
        query.execute((dir, fname, embedding, mtime)).unwrap();
    }

    /// returns online status if file exists, otherwise true to keep going
    async fn update_file(self: &Self, path: &Path, dir: &String) -> bool {
        let metadata = match path.metadata() {
            Ok(md) => md,
            Err(_) => {
                debug!("{:?} was deleted?", path);
                return true;
            },
        };
        let mtime = metadata.modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f32();
        let fname = path.file_name().unwrap().to_string_lossy();

        match self.read().await.indexer_mode {
            IndexerMode::Vector => {
                let stat = self.already_done_vector(dir, &fname, mtime).await;
                if stat == Entry::Done {
                    return true;
                }
                let cmd = format!("{} {}", self.read().await.cmd, shquote(path.to_string_lossy().as_ref()));
                match tokio::process::Command::new("sh").arg("-c").arg(&cmd).output().await {
                    Ok(out) => {
                        if !out.status.success() || out.stdout.len() == 0 {
                            error!("CMD FAILED {}: {}", cmd, unsafe { std::str::from_utf8_unchecked(&out.stderr) });
                            return self.indexer_online().await;
                        } else {
                            let embedding = out.stdout.clone();
                            trace!("{:?} VECTOR ({} bytes)", path, embedding.len());
                            self.save_vector(dir, &fname, &embedding, mtime, stat).await;
                            return true;
                        }
                    },
                    Err(e) => {error!("Process error: {}", e)},
                };
                return self.indexer_online().await;
            }
            IndexerMode::Text => {
                let stat = self.already_done_description(dir, &fname, mtime).await;
                if stat == Entry::Done {
                    return true;
                }
                let cmd = format!("{} {}", self.read().await.cmd, shquote(path.to_string_lossy().as_ref()));
                match tokio::process::Command::new("sh").arg("-c").arg(&cmd).output().await {
                    Ok(out) => {
                        if !out.status.success() || out.stdout.len() == 0 {
                            error!("CMD FAILED {}: {}", cmd, unsafe { std::str::from_utf8_unchecked(&out.stderr) });
                            return self.indexer_online().await;
                        } else {
                            let description = unsafe { std::str::from_utf8_unchecked(&out.stdout) };
                            trace!("{:?} DESC:{}", path, description.trim());
                            self.save_description(dir, &fname, &description, mtime, stat).await;
                            return true;
                        }
                    },
                    Err(e) => {error!("Process error: {}", e)},
                };
                return self.indexer_online().await;
            }
        }
    }

    async fn update_dir(self: &Self, dir: &String) -> DirResult {
        let local_ignore = if self.read().await.respect_gitignore {
            build_cumulative_gitignore(Path::new(dir))
        } else {
            gitignore::Gitignore::empty()
        };
        if let Match::Ignore(_) = local_ignore.matched(dir, true) {
            return DirResult::Ignore;
        }
        trace!("Updating dir:{}", dir);
        match std::fs::read_dir(dir) {
            Ok(read_dir) => {
                for dir_entry in read_dir {
                    if !self.read().await.idx_running {
                        break;
                    }
                    if let Ok(de) = dir_entry {
                        let path = de.path();
                        match path.extension() {
                            Some(ext) => {
                                if self.read().await.exts.contains(&ext.to_ascii_lowercase().to_string_lossy().as_ref()) {
                                    if let Match::Ignore(_) = self.read().await.search_ignore.matched(&path, path.is_dir()) {
                                        continue;
                                    }
                                    if let Match::Ignore(_) = local_ignore.matched(&path, false) {
                                        continue;
                                    }
                                    let mut online = true;
                                    let mut tries_left = 5;
                                    loop {
                                        if online && self.update_file(path.as_path(), dir).await {
                                            break;
                                        } else  {
                                            warn!("Retrying {:?} in a minute...", path);
                                            tries_left -= 1;
                                            sleep(Duration::from_secs(60)).await;
                                            online = self.indexer_online().await;
                                            if !online && tries_left == 0 {
                                                return DirResult::Fail;
                                            }
                                        }
                                    };
                                }
                            },
                            None => {},
                        }
                    }
                }
            },
            Err(e) => error!("Error reading dir {}: {}", dir, e),
        }
        return DirResult::Success;
    }

}


struct FilePicker {
    prev_path: String,
    prev_path_set_at: SystemTime,
    postproc_dir: String,
    postprocessor: String,
    def_save_dir: String,
    cmd: String,
    home: String,
    shtate: Arc<AsyncMtx<Shtate>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    use_prev: bool,
}

fn shquote(s: &str) -> String {
    if s.contains("\"") {
        return format!("'{}'", s);
    }
    return format!("\"{}\"", s);
}

#[derive(PartialEq)]
enum Entry {
    None,
    Old,
    Done,
}

#[derive(PartialEq)]
enum DirResult {
    Fail,
    Success,
    Ignore,
}

enum Section {
    FileChooser,
    Indexer,
    Global,
}

#[derive(Clone, PartialEq, Debug)]
enum IndexerMode {
    Text,
    Vector,
}

fn tilda<'a>(home: &String, dir: &'a str) -> Cow<'a,str> {
    if dir.trim_start().starts_with('~') {
        let expanded = dir.replacen("~", &home, 1);
        return Cow::from(expanded)
    }
    Cow::from(dir)
}

#[derive(Debug)]
struct Config {
    home: String,
    db_path: String,
    dbus_service: String,
    dbus_object_path: String,
    postproc_dir: String,
    postprocessor: String,
    def_save_dir: String,
    file_cmd: String,
    indexer_cmd: String,
    indexer_check: String,
    indexer_exts: String,
    indexer_enabled: bool,
    indexer_mode: IndexerMode,
    use_prev_path_for_save: bool,
}

impl Config {

    fn create_default_config(target_path: &std::path::Path) {
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).expect("Unable to create config directory");
        }
        let content = r#"
# off, error, warn, info, debug, trace
log_level = info

[filepicker]
cmd = /usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh
default_save_dir = ~/Downloads

# Use internally tracked path rather than the one provided by the client application
use_prev_path_for_save = false

# Point postprocessor to a script to automatically process files before upload.
# Replace the empty config value with the commented one to use the example script.
#postprocessor = /usr/share/xdg-desktop-portal-pikeru/postprocess.example.sh
postprocessor=
postprocess_dir = /tmp/pk_postprocess

[indexer]
# This section tells xdg-desktop-portal-pikeru how to build an index for semantic search.
# The example values here are for a caption generating server running on localhost that
# is used to generate searchable text for image files in any directory opened by pikeru.
# See how to install the caption server with indexer/caption_server/README.md in pikeru's
# git repo. It uses the same api as some version of stable diffusion webui, so you may use
# that instead if you want.
# Set log_level above to trace to see the searchable text results.

enable = false

# bash command that will be given an additional filepath arg and prints searchable text to stdout.
cmd = python /usr/share/xdg-desktop-portal-pikeru/img_indexer.py http://127.0.0.1:7860/sdapi/v1/interrogate

# bash command that only returns status code 0 when the indexer is online
check = curl http://127.0.0.1:7860/sdapi/v1/interrogate

# comma-separated list of file types that 'cmd' can process.
extensions = png,jpg,jpeg,gif,webp,tiff,bmp

# Indexing mode: "text" for fuzzy text search or "vector" for vector embedding search
mode = text
"#;
        fs::write(target_path, content.trim_start()).expect("Unable to create config file");
    }

    fn find_config() -> String {
        let home = std::env::var("HOME").unwrap();
        let xdg_home = std::env::var("XDG_CONFIG_HOME").unwrap_or("".to_string());
        let conf_home = Path::new(&home).join(".config").to_string_lossy().to_string();
        let sysconf = Path::new(&std::env::var("SYSCONFDIR").unwrap_or("/etc".to_string()))
            .join("xdg").to_string_lossy().to_string();
        let cdt = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or("Gnome".to_string());
        let mut filenames = cdt.split(':').collect::<Vec<&str>>();
        filenames.push("config");
        for dir in [&xdg_home, &conf_home, &sysconf] {
            if dir.is_empty() { continue; }
            for file in &filenames {
                let cpath = Path::new(dir).join("xdg-desktop-portal-pikeru").join(&file);
                if !cpath.is_file() {
                    continue;
                }
                return cpath.to_string_lossy().to_string();
            }
        }
        let xdg_home = std::env::var("XDG_CONFIG_HOME").unwrap_or(conf_home);
        let conf_path = Path::new(&xdg_home).join("xdg-desktop-portal-pikeru").join("config");
        eprintln!("No config file found, creating a new one.");
        Config::create_default_config(conf_path.as_path());
        conf_path.to_string_lossy().to_string()
    }

    fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = Options::new();
        opts.optopt("c", "config", "Path to config file", "PATH");
        opts.optopt("d", "db", "Path to index database (for testing)", "PATH");
        opts.optopt("l", "log", "Log level", "[off error warn info debug trace]");
        opts.optopt("s", "service", "D-Bus service name (tests only)", "NAME");
        opts.optopt("p", "path", "D-Bus object path (tests only)", "PATH");
        let parsed = match opts.parse(args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Bad args: {}", e);
                std::process::exit(1);
            }
        };
        let conf_path = parsed.opt_str("c").unwrap_or(Config::find_config());
        eprintln!("Conf path:{}", conf_path);
        let home = std::env::var("HOME").unwrap();
        let default_db = Path::new(&home).join(".cache").join("pikeru").join("index.db").to_string_lossy().into_owned();
        let db_path_override = parsed.opt_str("d");
        let mut postproc_dir = "/tmp/pk_postprocess".to_string();
        let mut def_save_dir = Path::new(&home).join("Downloads").to_string_lossy().to_string();
        let fp_cmds = ["/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/opt/pikeru/xdg_portal/contrib/pikeru-wrapper.sh"];
        let mut fp_cmd = fp_cmds.iter().find_map(|c|if Path::new(c).is_file() {Some(*c)} else {None})
            .unwrap_or(fp_cmds[0]).to_string();
        let mut postprocessor = "".to_string();
        let mut indexer_cmd = "".to_string();
        let mut indexer_check = "".to_string();
        let mut indexer_exts = "".to_string();
        let mut indexer_enabled = false;
        let mut indexer_mode = IndexerMode::Text;
        let mut use_prev_path_for_save = false;
        let mut log_level = "info".to_string();
        let mut dbus_service = String::from("org.freedesktop.impl.portal.desktop.pikeru");
        let mut dbus_object_path = String::from("/org/freedesktop/portal/desktop");
        let txt = match std::fs::read_to_string(&conf_path) {
            Ok(content) => content,
            Err(err) => {
                eprintln!("Error reading configuration file at {:?}: {}", conf_path, err);
                match err.kind() {
                    std::io::ErrorKind::NotFound => eprintln!("The configuration file does not exist."),
                    std::io::ErrorKind::PermissionDenied => eprintln!("Permission denied when trying to read the configuration file."),
                    _ => eprintln!("An unexpected error occurred while reading the configuration file."),
                }
                std::process::exit(1);
            }
        };
        let mut section = Section::Global;
        for line in txt.lines().map(|s|s.trim()).filter(|s|s.len()>0 && !s.starts_with('#')) {
            match line {
                "[filepicker]" => section = Section::FileChooser,
                "[indexer]" => section = Section::Indexer,
                _ => {
                    let (k, v) = str::split_once(line, '=').unwrap();
                    let (k, v) = (k.trim(), v.trim());
                    match section {
                        Section::Indexer => {
                            match k {
                                "cmd" => indexer_cmd = v.to_string(),
                                "check" => indexer_check = v.to_string(),
                                "extensions" => indexer_exts = v.to_string(),
                                "enable" => indexer_enabled = v.parse().unwrap_or(false),
                                "mode" => {
                                    match v {
                                        "text" => indexer_mode = IndexerMode::Text,
                                        "vector" => indexer_mode = IndexerMode::Vector,
                                        _ => eprintln!("Unknown indexer mode '{}', defaulting to 'text'", v),
                                    }
                                },
                                _ => eprintln!("Unknown indexer config value:{}", line),
                            }
                        },
                        Section::FileChooser => {
                            match k {
                                "cmd" => fp_cmd = v.to_string(),
                                "default_save_dir" => def_save_dir = v.to_string(),
                                "postprocess_dir" => postproc_dir = v.to_string(),
                                "postprocessor" => postprocessor = v.to_string(),
                                "use_prev_path_for_save" => use_prev_path_for_save = v.parse().unwrap(),
                                _ => eprintln!("Unknown filechooser config value:{}", line),
                            }
                        },
                        Section::Global => {
                            match k {
                                "log_level" => log_level = v.to_string(),
                                _ => {},
                            }
                        },
                    }
                }
            }
        }
        parsed.opt_str("l").map(|l|log_level = l);
        if let Some(s) = parsed.opt_str("s") { dbus_service = s; }
        if let Some(p) = parsed.opt_str("p") { dbus_object_path = p; }
        let ll = match log_level.as_str() {
            "off" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => { eprintln!("Unknown log level:{}. Defaulting to 'info'", log_level); LevelFilter::Info },
        };
        Builder::new().filter_level(ll).init();
        eprintln!("Log level: {}", ll);
        if !Path::new(&fp_cmd).is_file() {
            eprintln!("No filepicker executable found: {}", fp_cmd);
            std::process::exit(1);
        }
        Self {
            postproc_dir: tilda(&home, &postproc_dir).to_string(),
            postprocessor: tilda(&home, &postprocessor).to_string(),
            def_save_dir: tilda(&home, &def_save_dir).to_string(),
            file_cmd: tilda(&home, &fp_cmd).to_string(),
            indexer_cmd: tilda(&home, &indexer_cmd).to_string(),
            indexer_check: tilda(&home, &indexer_check).to_string(),
            indexer_exts,
            indexer_enabled,
            indexer_mode,
            home,
            db_path: db_path_override.unwrap_or(default_db),
            dbus_service,
            dbus_object_path,
            use_prev_path_for_save,
        }
    }
}

impl FilePicker {

    fn new(conf: &mut Config, shtate: Arc<AsyncMtx<Shtate>>, db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self {
            prev_path: conf.home.clone(),
            prev_path_set_at: SystemTime::now(),
            postproc_dir: take(&mut conf.postproc_dir),
            postprocessor: take(&mut conf.postprocessor),
            def_save_dir: take(&mut conf.def_save_dir),
            cmd: take(&mut conf.file_cmd),
            home: take(&mut conf.home),
            shtate,
            db,
            use_prev: conf.use_prev_path_for_save,
        }
    }

    async fn select_files(self: &mut Self, multi: bool, dir: bool, save: bool, path: &str) -> (u32, HashMap<String, OwnedValue>) {
        let dir = if dir   { 1 } else { 0 };
        let multi = if multi { 1 } else { 0 };
        let savenum = if save  { 1 } else { 0 };
        {
            const TIMEOUT: u64 = 60*60*24;
            if SystemTime::now().duration_since(self.prev_path_set_at).unwrap_or_default().as_secs() > TIMEOUT {
                self.prev_path = self.home.clone();
                self.prev_path_set_at = SystemTime::now();
            }
        }
        let cmd = if save {
            let final_path = if self.use_prev {
                let file_name = Path::new(path).file_name().map_or_else(|| "".to_string(), |s| s.to_string_lossy().to_string());
                Path::new(&self.prev_path).join(file_name).to_string_lossy().to_string()
            } else {
                path.to_string()
            };
            format!("PK_XDG=1 {} {} {} {} \"{}\"", self.cmd, multi, dir, savenum, tilda(&self.home,&final_path))
        } else {
            format!("PK_XDG=1 POSTPROCESS_DIR=\"{}\" POSTPROCESSOR=\"{}\" {} {} {} {} {}",
                    self.postproc_dir, self.postprocessor, self.cmd, multi, dir, savenum,
                    shquote(tilda(&self.home,&self.prev_path).as_ref()))
        };
        self.db.lock().unwrap().cache_flush().unwrap();
        debug!("CMD:{}", cmd);
        let output = match tokio::process::Command::new("sh").arg("-c").arg(cmd).output().await {
            Ok(out) => {
                if out.stderr.len() > 0 {
                    let txt = unsafe { std::str::from_utf8_unchecked(&out.stderr) };
                    if out.status.success() {
                        info!("From filepicker:{}", txt);
                    } else {
                        error!("From filepicker:{}", txt);
                    }
                }
                unsafe { std::str::from_utf8_unchecked(&out.stdout).to_owned() }
            },
            Err(e) => {eprintln!("Process error: {}", e); "".to_owned()},
        };
        let mut gotfirst = false;
        let mut arr = Vec::new();
        let mut builder = gitignore::GitignoreBuilder::new("");
        self.shtate.lock().await.current_searchignore.lines().for_each(|line|{
            let _ = builder.add_line(None, line);
        });
        let ignorer = builder.build().ok();
        for line in output.lines() {
            if !gotfirst {
                gotfirst = true;
                if let Some(par_dir) = self.get_dir(line) {
                    let mut update_prevpath = true;
                    if let Some(ref gi) = ignorer {
                        update_prevpath = match gi.matched(&par_dir, true) {Match::Ignore(_) => false, _ => true};
                    }
                    if update_prevpath {
                        self.prev_path = par_dir;
                        self.prev_path_set_at = SystemTime::now();
                    }
                }
            }
            trace!("Selected: {}", line);
            arr.push(format!("file://{}", line));
        }
        let mut ret = HashMap::new();
        let status = if arr.is_empty() { 1 } else {
            ret.insert("uris".to_string(), Value::from(arr).try_to_owned().unwrap());
            0
        };
        (status, ret)
    }

    fn get_dir(self: &Self, path: &str) -> Option<String> {
        let p = Path::new(path);
        let parent = p.parent()?;
        let ps =  parent.to_string_lossy();
        if !parent.is_dir() || ps == self.postproc_dir {
            return None;
        }
        Some(ps.to_string()) 
    }

}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FilePicker {
    async fn open_file(&mut self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let dir = match options.get("directory").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { error!("DIR type error"); false},
        };
        let multi = match options.get("multiple").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { error!("MULTI type error"); false},
        };
        self.select_files(multi, dir, false, "/").await
    }

    async fn save_file(&mut self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let dir = match options.get("current_folder").unwrap_or(&Value::from(&self.def_save_dir)) {
            Value::Array(s) => {
                let b = to_bytes(Context::new_dbus(LE, 0), s).unwrap();
                match std::str::from_utf8(&b[4..b.len()-1]) {
                    Ok(s) => s.to_string(),
                    Err(e) => {
                        error!("Error reading dir:{}", e);
                        self.def_save_dir.clone()
                    },
                }
            },
            _ => self.def_save_dir.clone(),
        };
        let fname = match options.get("current_name").unwrap_or(&Value::from("download")) {
            Value::Str(s) => s.to_string(),
            _ => "download".to_string(),
        };
        let path = Path::new(&dir).join(fname);
        self.select_files(false, false, true, &path.to_string_lossy()).await
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut config = Config::new();
    eprintln!("Running {:#?}", config);
    let idxfile = Path::new(&config.db_path).to_owned();
    std::fs::create_dir_all(idxfile.parent().unwrap()).unwrap();
    let db = Arc::new(std::sync::Mutex::new(rusqlite::Connection::open(&idxfile).unwrap()));
    let sht = Arc::new(AsyncMtx::new(Shtate::default()));
    let picker = FilePicker::new(&mut config, sht.clone(), db.clone());
    let indexer = Indexer::new(sht.clone(), &mut config, db);
    let service_name = config.dbus_service.clone();
    let object_path = config.dbus_object_path.clone();
    eprintln!("D-Bus: {} @ {}", service_name, object_path);
    let obj = ObjectPath::from_string_unchecked(object_path.clone());
    let _conn = connection::Builder::session()?
        .name(service_name)?
        .serve_at(&obj, picker)?
        .serve_at(&obj, indexer)?
        .build()
        .await?;
    pending::<()>().await;
    Ok(())
}
