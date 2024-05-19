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
    path::Path
};

struct Indexer;
#[interface(name = "org.freedesktop.impl.portal.SearchIndexer")]
impl Indexer {
   async fn update(&mut self, paths: Vec<&str>, recursive: bool) -> String {
        let ret = format!("Updating index for {:?}. Recursive: {}", paths, recursive);
        eprintln!("{}",ret);
        ret
    }
}

#[derive(Debug)]
struct FilePicker {
    prev_path: String,
    postproc_dir: String,
    def_save_dir: String,
    cmd: String,
    home: String,
}
enum Section {
    FileChooser,
    Other,
}
fn tilda<'a>(home: &String, dir: &'a str) -> Cow<'a,str> {
    if dir.contains('~') {
        let expanded = dir.replace("~", &home);
        return Cow::from(expanded)
    }
    Cow::from(dir)
}
impl FilePicker {

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
        let cmds = ["/usr/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh",
                    "/opt/pikeru/xdg_portal/contrib/pikeru-wrapper.sh"];
        let mut cmd = cmds.iter().find_map(|c|if Path::new(c).is_file() {Some(*c)} else {None})
            .unwrap_or(cmds[0]).to_string();
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
                        _ => match section {
                            Section::FileChooser => {
                                let (k, v) = str::split_once(line, '=').unwrap();
                                let (k, v) = (k.trim(), v.trim());
                                match k {
                                    "cmd" => cmd = v.to_string(),
                                    "default_dir" => def_save_dir = v.to_string(),
                                    "postprocess_dir" => postproc_dir = v.to_string(),
                                    _ => {},
                                }
                            },
                            Section::Other => {},
                        }
                    }
                }
                break;
            }
        }
        if !Path::new(&cmd).is_file() {
            eprintln!("No filepicker executable found: {}", cmd);
            std::process::exit(1);
        }
        Self {
            prev_path: home.clone(),
            postproc_dir: tilda(&home, &postproc_dir).to_string(),
            def_save_dir: tilda(&home, &def_save_dir).to_string(),
            cmd: tilda(&home, &cmd).to_string(),
            home,
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
        let output = match tokio::process::Command::new("sh").arg("-c").arg(cmd).output().await {
            Ok(out) => {
                eprintln!("{}", unsafe { std::str::from_utf8_unchecked(&out.stderr) });
                unsafe { std::str::from_utf8_unchecked(&out.stdout).to_owned() }
            },
            Err(e) => {eprintln!("Process error: {}", e); "".to_owned()},
        };
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
    let picker = FilePicker::new();
    let indexer = Indexer{};
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
