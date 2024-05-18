//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use std::{error::Error, future::pending};
use zbus::{
    connection, interface,
    zvariant::{Value,OwnedValue}
};
use std::collections::HashMap;


struct FilePicker;

fn select_file(dir: bool, multi: bool, save: bool, path: &str) -> (u32, HashMap<String, OwnedValue>) {
    let dir   = if dir   { 1 } else { 0 };
    let multi = if multi { 1 } else { 0 };
    let save  = if save  { 1 } else { 0 };
    let cmd = format!("/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh {} {} {} {}",
                      multi, dir, save, path);
    let output = match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
        Ok(out) => unsafe { std::str::from_utf8_unchecked(&out.stdout).to_owned() },
        Err(e) => {eprintln!("Process error: {}", e); "".to_owned()},
    };
    let arr = output.lines().map(|line|format!("file://{}",line)).collect::<Vec<_>>();
    let mut ret = HashMap::new();
    let status = if arr.is_empty() { 1 } else {
        let uris = Value::from(arr).try_to_owned().unwrap();
        ret.insert("uris".to_string(), uris);
        0
    };
    (status, ret)
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FilePicker {
    fn open_file(&self, _ob: zbus::zvariant::ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        let multi = options.get("multiple").unwrap_or(&Value::Bool(false));
        let dir = options.get("directory").unwrap_or(&Value::Bool(false));
        let dir = match dir {
            &Value::Bool(b) => b,
            _ => { eprintln!("DIR type error"); false},
        };
        let multi = match multi {
            &Value::Bool(b) => b,
            _ => { eprintln!("MULTI type error"); false},
        };
        select_file(multi, dir, false, "/home/d")
    }

    fn save_file(&self, _ob: zbus::zvariant::ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        eprint!("Save options:{:#?}", options);
        let fname = match options.get("current_name") {
            Some(s) => s.to_string(),
            None => "download".to_string(),
        };
        let path = format!("/home/d/{}", fname);
        select_file(false, false, true, path.as_str())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    eprintln!("portal running");
    let picker = FilePicker {};
    let _conn = connection::Builder::session()?
        .name("org.freedesktop.impl.portal.desktop.pikeru")?
        .serve_at("/org/freedesktop/portal/desktop", picker)?
        .build()
        .await?;

    pending::<()>().await;

    Ok(())
}
