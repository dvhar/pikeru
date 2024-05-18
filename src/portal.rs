//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use zbus::{
    connection, interface,
    zvariant::{Value,OwnedValue,ObjectPath,
    //to_bytes,LE,serialized::Context
    }
};
use std::{
    error::Error, future::pending,
    collections::HashMap,
    path::Path
};


struct FilePicker {
    prev_path: String,
    postproc_dir: String,
    def_save_dir: String,
    cmd: String,
}

impl<'a> FilePicker {
    fn new() -> Self {
        Self {
            prev_path: "".to_string(),
            postproc_dir: "/tmp/pk_postprocess".to_string(),
            def_save_dir: "/home/d/Downloads".to_string(),
            cmd: "/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh".to_string(),
        }
    }

    fn select_files(self: &mut Self, dir: bool, multi: bool, save: bool, path: &str) -> (u32, HashMap<String, OwnedValue>) {
        let dir   = if dir   { 1 } else { 0 };
        let multi = if multi { 1 } else { 0 };
        let save  = if save  { 1 } else { 0 };
        let cmd = format!("{} {} {} {} {}", self.cmd, multi, dir, save, path);
        eprintln!("CMD:{}", cmd);
        let output = match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
            Ok(out) => unsafe { std::str::from_utf8_unchecked(&out.stdout).to_owned() },
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
    fn open_file(&mut self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let dir = match options.get("directory").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { eprintln!("DIR type error"); false},
        };
        let multi = match options.get("multiple").unwrap_or(&Value::Bool(false)) {
            &Value::Bool(b) => b,
            _ => { eprintln!("MULTI type error"); false},
        };
        self.select_files(multi, dir, false, "/home/d")
    }

    fn save_file(&mut self, _ob: ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let dir = match options.get("current_folder").unwrap_or(&Value::from(&self.def_save_dir)) {
            Value::Str(s) => s.to_string(),
            //Value::Array(s) => {
                //let ctx = Context::new_dbus(LE, 0);
                //let b = to_bytes(ctx, s).unwrap();
                //eprintln!("WTF:{:?}", b);
                //match std::str::from_utf8(&b[4..]) {
                    //Ok(s) => s.to_string(),
                    //Err(e) => {
                        //eprintln!("Error reading dir:{}", e);
                        //self.def_save_dir.clone()
                    //},
                //}
            //},
            _ => self.def_save_dir.clone(),
        };
        let fname = match options.get("current_name").unwrap_or(&Value::from("download")) {
            Value::Str(s) => s.to_string(),
            _ => "download".to_string(),
        };
        eprintln!("DIR:{} FNAME:{}", dir, fname);
        let path = Path::new(&dir).join(fname);
        eprintln!("PATH: {:?}", path);
        self.select_files(false, false, true, &path.to_string_lossy())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    eprintln!("portal running");
    let picker = FilePicker::new();
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.impl.portal.desktop.pikeru")?
        .serve_at("/org/freedesktop/portal/desktop", picker)?
        .build()
        .await?;
    pending::<()>().await;
    Ok(())
}
