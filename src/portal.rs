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
    error::Error, future::pending,
    collections::HashMap,
    borrow::Cow,
    path::Path,
    mem::take,
    sync::{Arc,Mutex},
};
use rusqlite;
use tokio::{
    sync::mpsc::{
        UnboundedReceiver as UReceiver,
        UnboundedSender as USender,
        unbounded_channel,
    },
    time,
    time::sleep,
    time::Duration,

};
use log::{info,trace,error,debug,warn,LevelFilter};
use env_logger::Builder;
use ctrlc;
use ignore::{gitignore,Match};


#[derive(Default, Debug)]
struct Shtate {
    idx_running: bool,
    picker_open: bool,
    paused: bool,
}

enum Msg {
    Start,
    Dirs(Vec<String>),
    Ignore(String),
}

struct IdxManager {
    shtate: Arc<Mutex<Shtate>>,
    cmd: String,
    check: String,
    exts: Vec<&'static str>,
    con: Arc<Mutex<rusqlite::Connection>>,
    ignore: gitignore::Gitignore,
    igtxt: String,
}

async fn index_loop(mut mgr: IdxManager, mut chan: UReceiver<Msg>, enabled: bool) {
    let mut done_map = HashMap::<String,bool>::new();
    let mut timeout = time::Instant::now().checked_add(time::Duration::from_secs(60)).unwrap();
    let mut online = mgr.indexer_online().await;
    info!("indexer {}", if online {"online"} else {"offline"});
    loop {
        let msg = chan.recv().await.unwrap();
        if !enabled { continue; }
        let uptodate = timeout.cmp(&time::Instant::now()) == core::cmp::Ordering::Greater;
        if !uptodate {
            timeout = timeout.checked_add(time::Duration::from_secs(60)).unwrap();
            online = mgr.indexer_online().await;
            if !online { warn!("indexer offline"); }
        }
        if !online {
            continue;
        }
        match msg {
            Msg::Start => {
                if !mgr.shtate.lock().unwrap().paused {
                    debug!("Starting index");
                }
                mgr.shtate.lock().unwrap().idx_running = true;
                while !mgr.shtate.lock().unwrap().picker_open {
                    if let Some(dir) = done_map.iter().find(|v|!v.1) {
                        if mgr.update_dir(dir.0).await {
                            done_map.entry(dir.0.to_string()).and_modify(|v| *v = true);
                        } else {
                            error!("Indexing batch failed");
                            done_map.clear();
                            break;
                        }
                    } else {
                        debug!("Indexing batch finished");
                        done_map.clear();
                        break;
                    }
                }
                mgr.shtate.lock().unwrap().idx_running = false;
            },
            Msg::Dirs(dirs) => {
                debug!("Got dirs");
                dirs.into_iter().for_each(|dir|{done_map.entry(dir).or_default();});
            },
            Msg::Ignore(txt) => {
                mgr.update_ignore(txt);
            }
        }
    }
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

impl IdxManager {

    fn new(shtate: Arc<Mutex<Shtate>>,
           config: &mut Config,
           con: Arc<Mutex<rusqlite::Connection>>) -> Self {
        match con.lock() {
           Ok(c) => { 
            c.execute("create table if not exists descriptions
                      (fname text, dir text, description text, mtime real);", ()).unwrap();
            c.pragma_update(None, "journal_mode", "WAL").unwrap();
           },
           Err(e) => eprintln!("{}", e),
        }
        let con2 = con.clone();
        ctrlc::set_handler(move || {
            if let Err(e) = con2.lock().unwrap().cache_flush() {
                eprintln!("failed to flush index db:{}", e);
            }
            eprintln!("Portal closing");
            std::process::exit(0);
        }).expect("Error setting Ctrl-C handler");
        Self {
            shtate,
            cmd: take(&mut config.indexer_cmd),
            check: take(&mut config.indexer_check),
            exts: Box::new(take(&mut config.indexer_exts)).leak().split(',').collect(),
            con,
            ignore: gitignore::Gitignore::new("").0,
            igtxt: String::new(),
        }
    }

    fn update_ignore(self: &mut Self, txt: String) {
        if txt == self.igtxt {
            return
        }
        let mut builder = gitignore::GitignoreBuilder::new("");
        txt.lines().for_each(|line|{builder.add_line(None, line).unwrap();});
        self.ignore = builder.build().unwrap();
        self.igtxt = txt;
    }

    async fn indexer_online(self: &Self) -> bool {
        match tokio::process::Command::new("sh").arg("-c").arg(&self.check).output().await {
            Ok(out) => out.status.success(),
            Err(_) => false,
        }
    }

    fn already_done(self: &Self, dir: &String, fname: &str, mtime: f32) -> Entry {
        let con = self.con.lock().unwrap();
        let mut query = con.prepare("select mtime from descriptions where dir = ?1 and fname = ?2").unwrap();
        let ret = match query.query([dir.as_str(), fname.as_ref()]).unwrap().next().unwrap() {
            Some(r) => {
                let prev_time: f32 = r.get(0).unwrap();
                match prev_time == mtime {
                    true => Entry::Done,
                    false => Entry::Old,
                }
            },
            None => Entry::None,
        };
        ret
    }

    fn save(self: &Self, dir: &String, fname: &str, desc: &str, mtime: f32, stat: Entry) {
        let con = self.con.lock().unwrap();
        let mut query = con.prepare(match stat {
            Entry::None => "insert into descriptions (dir, fname, description, mtime) values (?1, ?2, ?3, ?4)",
            Entry::Old => "update descriptions set description = ?3, mtime = ?4 where dir = ?1 and fname = ?2",
            Entry::Done => unreachable!(),
        }).unwrap();
        query.execute((dir, fname, desc, mtime)).unwrap();
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
        let stat = self.already_done(dir, &fname, mtime);
        if stat == Entry::Done {
            return true;
        }
        let cmd = format!("{} {}", self.cmd, shquote(path.to_string_lossy().as_ref()));
        match tokio::process::Command::new("sh").arg("-c").arg(&cmd).output().await {
            Ok(out) => {
                if !out.status.success() || out.stdout.len() == 0 {
                    error!("CMD FAILED {}: {}", cmd, unsafe { std::str::from_utf8_unchecked(&out.stderr) });
                    return self.indexer_online().await;
                } else {
                    let description = unsafe { std::str::from_utf8_unchecked(&out.stdout) };
                    trace!("{:?} DESC:{}", path, description.trim());
                    self.save(dir, &fname, &description, mtime, stat);
                    return true;
                }
            },
            Err(e) => {error!("Process error: {}", e)},
        };
        return self.indexer_online().await;
    }

    /// returns false if giving up
    async fn update_dir(self: &Self, dir: &String) -> bool {
        trace!("Updating dir:{}", dir);
        match std::fs::read_dir(dir) {
            Ok(read_dir) => {
                for dir_entry in read_dir {
                    let path = dir_entry.unwrap().path();
                    match path.extension() {
                        Some(ext) => {
                            if self.exts.contains(&ext.to_ascii_lowercase().to_string_lossy().as_ref()) {
                                match self.ignore.matched(&path, path.is_dir()) {
                                    Match::Ignore(_) => continue,
                                    _ => {},
                                }
                                let mut online = true;
                                let mut tries_left = 10;
                                loop {
                                    if self.shtate.lock().unwrap().paused {
                                        sleep(Duration::from_secs(60)).await;
                                        continue;
                                    }
                                    if online && self.update_file(path.as_path(), dir).await {
                                        break;
                                    } else  {
                                        warn!("Retrying {:?} in a minute...", path);
                                        tries_left -= 1;
                                        sleep(Duration::from_secs(60)).await;
                                        online = self.indexer_online().await;
                                        if !online && tries_left == 0 {
                                            return false;
                                        }
                                    }
                                };
                            }
                        },
                        None => {},
                    }
                }
            },
            Err(e) => error!("Error reading dir {}: {}", dir, e),
        }
        return true;
    }

}

#[allow(dead_code)]
struct Indexer {
    tx: USender<Msg>,
    shtate: Arc<Mutex<Shtate>>,
    con: Arc<Mutex<rusqlite::Connection>>,
}

#[interface(name = "org.freedesktop.impl.portal.SearchIndexer")]
impl Indexer {
    async fn pause_resume(&self, active: bool) {
        if active {
            eprintln!("Resumed indexer");
        } else {
            eprintln!("Paused indexer");
        }
        self.shtate.lock().unwrap().paused = !active;
    }
    async fn update(&self, dirs: Vec<String>) {
        self.tx.send(Msg::Dirs(dirs)).unwrap(); 
        let st = self.shtate.lock().unwrap();
        if !st.idx_running && !st.picker_open {
            self.tx.send(Msg::Start).unwrap();
        }
    }
    async fn configure(&mut self, _respect_gitignore: bool, ignore: String) {
        trace!("Got gitignore configure request");
        self.tx.send(Msg::Ignore(ignore)).unwrap();
    }

}
impl Indexer {
    fn new(tx: USender<Msg>, shtate: Arc<Mutex<Shtate>>, con: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self {
            tx,
            shtate,
            con,
        }
    }
}

struct FilePicker {
    prev_path: Mutex<String>,
    postproc_dir: String,
    postprocessor: String,
    def_save_dir: String,
    cmd: String,
    home: String,
    shtate: Arc<Mutex<Shtate>>,
    db: Arc<Mutex<rusqlite::Connection>>,
    tx: USender<Msg>,
}
enum Section {
    FileChooser,
    Indexer,
    Global,
}
fn tilda<'a>(home: &String, dir: &'a str) -> Cow<'a,str> {
    if dir.contains('~') {
        let expanded = dir.replace("~", &home);
        return Cow::from(expanded)
    }
    Cow::from(dir)
}

#[derive(Debug)]
struct Config {
    home: String,
    prev_path: String,
    postproc_dir: String,
    postprocessor: String,
    def_save_dir: String,
    filecmd: String,
    indexer_cmd: String,
    indexer_check: String,
    indexer_exts: String,
    indexer_enabled: bool,
}

impl Config {

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
            for file in &filenames {
                let cpath = Path::new(dir).join("xdg-desktop-portal-pikeru").join(&file);
                if !cpath.is_file() {
                    continue;
                }
                return cpath.to_string_lossy().to_string();
            }
        }
        eprintln!("No config file");
        String::new()
    }

    fn new() -> Self {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut opts = Options::new();
        opts.optopt("c", "config", "Path to config file", "PATH");
        opts.optopt("l", "log", "Log level", "[off error warn info debug trace]");
        let matches = match opts.parse(args) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Bad args: {}", e);
                std::process::exit(1);
            }
        };
        let conf_path = matches.opt_str("c").unwrap_or(Config::find_config());
        eprintln!("Conf path:{}", conf_path);
        let home = std::env::var("HOME").unwrap();
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
        let mut log_level = "info".to_string();
        let txt = std::fs::read_to_string(conf_path).unwrap();
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
                                "enable" => indexer_enabled = v.parse().unwrap(),
                                _ => eprintln!("Unknown indexer config value:{}", line),
                            }
                        },
                        Section::FileChooser => {
                            match k {
                                "cmd" => fp_cmd = v.to_string(),
                                "default_save_dir" => def_save_dir = v.to_string(),
                                "postprocess_dir" => postproc_dir = v.to_string(),
                                "postprocessor" => postprocessor = v.to_string(),
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
        matches.opt_str("l").map(|l|log_level = l);
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
            prev_path: home.clone(),
            postproc_dir: tilda(&home, &postproc_dir).to_string(),
            postprocessor: tilda(&home, &postprocessor).to_string(),
            def_save_dir: tilda(&home, &def_save_dir).to_string(),
            filecmd: tilda(&home, &fp_cmd).to_string(),
            indexer_cmd: tilda(&home, &indexer_cmd).to_string(),
            indexer_check: tilda(&home, &indexer_check).to_string(),
            indexer_exts,
            indexer_enabled,
            home,
        }
    }
}

impl FilePicker {

    fn new(conf: &mut Config, shtate: Arc<Mutex<Shtate>>, tx: USender<Msg>, db: Arc<Mutex<rusqlite::Connection>>) -> Self {
        Self {
            prev_path: Mutex::new(take(&mut conf.prev_path)),
            postproc_dir: take(&mut conf.postproc_dir),
            postprocessor: take(&mut conf.postprocessor),
            def_save_dir: take(&mut conf.def_save_dir),
            cmd: take(&mut conf.filecmd),
            home: take(&mut conf.home),
            shtate,
            db,
            tx,
        }
    }

    async fn select_files(self: &Self, multi: bool, dir: bool, save: bool, path: &str) -> (u32, HashMap<String, OwnedValue>) {
        let dir = if dir   { 1 } else { 0 };
        let multi = if multi { 1 } else { 0 };
        let savenum = if save  { 1 } else { 0 };
        let cmd = if save {
            format!("{} {} {} {} \"{}\"", self.cmd, multi, dir, savenum, tilda(&self.home,path))
        } else {
            format!("POSTPROCESS_DIR=\"{}\" POSTPROCESSOR=\"{}\" {} {} {} {} {}",
                    self.postproc_dir, self.postprocessor, self.cmd, multi, dir, savenum,
                    shquote(tilda(&self.home,&self.prev_path.lock().unwrap()).as_ref()))
        };
        self.db.lock().unwrap().cache_flush().unwrap();
        debug!("CMD:{}", cmd);
        self.shtate.lock().unwrap().picker_open = true;
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
        match self.shtate.lock() {
            Ok(mut mtx) => {
                mtx.picker_open = false;
                if !mtx.idx_running {
                    self.tx.send(Msg::Start).unwrap();
                }
            },
            Err(e) => eprintln!("MTX error: {}", e),
        }
        let mut gotfirst = false;
        let arr = output.lines().map(|line| {
            if !gotfirst {
                gotfirst = true;
                if let Some(par_dir) = self.get_dir(line) {
                   *self.prev_path.lock().unwrap() = par_dir;
                }
            }
            format!("file://{}",line)
        }).collect::<Vec<_>>();
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
    async fn open_file(&self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
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

    async fn save_file(&self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
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
    let idxfile = Path::new(&config.home).join(".cache").join("pikeru").join("index.db");
    let sht = Arc::new(Mutex::new(Shtate::default()));
    let (tx, rx) = unbounded_channel::<Msg>();
    std::fs::create_dir_all(idxfile.parent().unwrap()).unwrap();
    let db = Arc::new(Mutex::new(rusqlite::Connection::open(idxfile).unwrap()));
    let picker = FilePicker::new(&mut config, sht.clone(), tx.clone(), db.clone());
    let indexer = Indexer::new(tx, sht.clone(), db.clone());
    let manager = IdxManager::new(sht.clone(), &mut config, db);
    tokio::spawn(index_loop(manager, rx, config.indexer_enabled));
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.impl.portal.desktop.pikeru")?
        .serve_at("/org/freedesktop/portal/desktop", picker)?
        .serve_at("/org/freedesktop/portal/desktop", indexer)?
        .build()
        .await?;
    pending::<()>().await;
    Ok(())
}
