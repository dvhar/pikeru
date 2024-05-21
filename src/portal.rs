//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
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
use tokio::sync::mpsc::{
    UnboundedReceiver as UReceiver,
    UnboundedSender as USender,
    unbounded_channel,
};

#[derive(Default, Debug)]
struct Shtate {
    idx_running: bool,
    picker_open: bool,
}

enum Msg {
    Start,
    Dirs(Vec<String>),
}

async fn index_loop(shtate: Arc<Mutex<Shtate>>,
                    _cmd: String,
                    _check: String,
                    exts: Vec<&'static str>,
                    mut chan: UReceiver<Msg>) {
    let con = rusqlite::Connection::open("/tmp/pk_index.db").unwrap();
    con.execute("create table if not exists descriptions
                (fname text, dir text, description text, mtime real);", ()).unwrap();
    let mut map = HashMap::<String,bool>::new();
    loop {
        let msg = chan.recv().await.unwrap();
        match msg {
            Msg::Start => {
                eprintln!("Starting index");
                shtate.lock().unwrap().idx_running = true;
                while !shtate.lock().unwrap().picker_open {
                    if let Some(dir) = map.iter().find(|v|!v.1) {
                        update_dir(dir.0, &exts).await;
                        map.entry(dir.0.to_string()).and_modify(|v| *v = true);
                    } else {
                        map.clear();
                        break;
                    }
                }
                shtate.lock().unwrap().idx_running = false;
            },
            Msg::Dirs(dirs) => {
                eprintln!("got dirs: {:?}", dirs);
                dirs.into_iter().for_each(|dir|{map.entry(dir).or_default();});
            },
        }
    }
}

async fn update_file(path: &Path) {
    //let ext = path.extension().unwrap_or("".into());
    eprintln!("PATH:{}", path.to_string_lossy());
    //let md = path.metadata().unwrap();
    //let date = md.mtime();
}
async fn update_dir(dir: &String, exts: &Vec<&'static str>) {
          eprintln!("INDEXING DIR:{}", dir);          
    return;
    match std::fs::read_dir(dir) {
        Ok(rd) => {
            for f in rd {
                let path = f.unwrap().path();
                match path.extension() {
                    Some(ext) => {
                        //if self.exts.contains(&ext.to_ascii_lowercase().to_string_lossy().as_ref()) {
                            //self.update_file(path.as_path()).await;
                        //}
                    },
                    None => {},
                }
            }
        },
        Err(e) => eprintln!("Error reading dir {}: {}", dir, e),
    }
}

struct Indexer {
    tx: USender<Msg>,
}
#[interface(name = "org.freedesktop.impl.portal.SearchIndexer")]
impl Indexer {
    async fn update(&self, dirs: Vec<String>) -> () {
        self.tx.send(Msg::Dirs(dirs)).unwrap(); 
    }

}
impl Indexer {
    fn new(tx: USender<Msg>) -> Self {
        Self {
            tx,
        }
    }
}

#[derive(Debug)]
struct FilePicker {
    prev_path: String,
    postproc_dir: String,
    def_save_dir: String,
    cmd: String,
    home: String,
    shtate: Arc<Mutex<Shtate>>,
    tx: USender<Msg>,
}
enum Section {
    FileChooser,
    Indexer,
    Other,
}
fn tilda<'a>(home: &String, dir: &'a str) -> Cow<'a,str> {
    if dir.contains('~') {
        let expanded = dir.replace("~", &home);
        return Cow::from(expanded)
    }
    Cow::from(dir)
}

struct Config {
    home: String,
    prev_path: String,
    postproc_dir: String,
    def_save_dir: String,
    filecmd: String,
    indexer_cmd: String,
    indexer_check: String,
    indexer_exts: String,
}
impl Config {
    fn new() -> Self {
        let home = std::env::var("HOME").unwrap();
        let xdg_home = std::env::var("XDG_CONFIG_HOME").unwrap_or("".to_string());
        let conf_home = Path::new(&home).join(".config").to_string_lossy().to_string();
        let sysconf = Path::new(&std::env::var("SYSCONFDIR").unwrap_or("/etc".to_string()))
            .join("xdg").to_string_lossy().to_string();
        let cdt = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or("Gnome".to_string());
        let mut filenames = cdt.split(':').collect::<Vec<&str>>();
        filenames.push("config");
        let mut postproc_dir = "/tmp/pk_postprocess".to_string();
        let mut def_save_dir = Path::new(&home).join("Downloads").to_string_lossy().to_string();
        let fp_cmds = ["/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/opt/pikeru/xdg_portal/contrib/pikeru-wrapper.sh"];
        let mut fp_cmd = fp_cmds.iter().find_map(|c|if Path::new(c).is_file() {Some(*c)} else {None})
            .unwrap_or(fp_cmds[0]).to_string();
        let mut indexer_cmd = "".to_string();
        let mut indexer_check = "".to_string();
        let mut indexer_exts = "".to_string();
        for dir in [&xdg_home, &conf_home, &sysconf] {
            for file in &filenames {
                let cpath = Path::new(dir).join("xdg-desktop-portal-pikeru").join(&file);
                if !cpath.is_file() {
                    continue;
                }
                let txt = std::fs::read_to_string(cpath).unwrap();
                let mut section = Section::Other;
                for line in txt.lines().map(|s|s.trim()).filter(|s|s.len()>0 && !s.starts_with('#')) {
                    match line {
                        "[filechooser]" => section = Section::FileChooser,
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
                                        _ => eprintln!("Unknown indexer config value:{}", line),
                                    }
                                },
                                Section::FileChooser => {
                                    match k {
                                        "cmd" => fp_cmd = v.to_string(),
                                        "default_dir" => def_save_dir = v.to_string(),
                                        "postprocess_dir" => postproc_dir = v.to_string(),
                                        "indexer" => indexer_cmd = v.to_string(),
                                        _ => eprintln!("Unknown filechooser config value:{}", line),
                                    }
                                },
                                Section::Other => {},
                            }
                        }
                    }
                }
                break;
            }
        }
        if !Path::new(&fp_cmd).is_file() {
            eprintln!("No filepicker executable found: {}", fp_cmd);
            std::process::exit(1);
        }
        Self {
            prev_path: home.clone(),
            postproc_dir: tilda(&home, &postproc_dir).to_string(),
            def_save_dir: tilda(&home, &def_save_dir).to_string(),
            filecmd: tilda(&home, &fp_cmd).to_string(),
            indexer_cmd: tilda(&home, &indexer_cmd).to_string(),
            indexer_check,
            indexer_exts,
            home,
        }
    }
}

impl FilePicker {

    fn new(conf: &mut Config, shtate: Arc<Mutex<Shtate>>, tx: USender<Msg>) -> Self {
        Self {
            prev_path: take(&mut conf.prev_path),
            postproc_dir: take(&mut conf.postproc_dir),
            def_save_dir: take(&mut conf.def_save_dir),
            cmd: take(&mut conf.filecmd),
            home: take(&mut conf.home),
            shtate,
            tx,
        }
    }

    async fn select_files(self: &mut Self, multi: bool, dir: bool, save: bool, path: &str) -> (u32, HashMap<String, OwnedValue>) {
        let dir = if dir   { 1 } else { 0 };
        let multi = if multi { 1 } else { 0 };
        let savenum = if save  { 1 } else { 0 };
        let cmd = if save {
            format!("{} {} {} {} \"{}\"", self.cmd, multi, dir, savenum, tilda(&self.home,path))
        } else {
            format!("POSTPROCESS_DIR=\"{}\" {} {} {} {} \"{}\"",
                    self.postproc_dir, self.cmd, multi, dir, savenum, tilda(&self.home,&self.prev_path))
        };
        eprintln!("CMD:{}", cmd);
        self.shtate.lock().unwrap().picker_open = true;
        let output = match tokio::process::Command::new("sh").arg("-c").arg(cmd).output().await {
            Ok(out) => {
                eprintln!("{}", unsafe { std::str::from_utf8_unchecked(&out.stderr) });
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
                    self.prev_path = par_dir;
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
    async fn open_file(&mut self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let dir = match options.get("directory").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { eprintln!("DIR type error"); false},
        };
        let multi = match options.get("multiple").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { eprintln!("MULTI type error"); false},
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
                        eprintln!("Error reading dir:{}", e);
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
    let sht = Arc::new(Mutex::new(Shtate::default()));
    let (tx, rx) = unbounded_channel::<Msg>();
    tokio::spawn(index_loop(sht.clone(),
                            take(&mut config.indexer_cmd),
                            take(&mut config.indexer_check),
                            Box::new(take(&mut config.indexer_exts)).leak().split(',').collect(),
                            rx));
    let picker = FilePicker::new(&mut config, sht.clone(), tx.clone());
    let indexer = Indexer::new(tx);
    eprintln!("Running {:#?}", picker);
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.impl.portal.desktop.pikeru")?
        .serve_at("/org/freedesktop/portal/desktop", picker)?
        .serve_at("/org/freedesktop/portal/desktop", indexer)?
        .build()
        .await?;
    pending::<()>().await;
    Ok(())
}
