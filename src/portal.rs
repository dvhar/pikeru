//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use std::{error::Error, future::pending};
use zbus::{
    connection, interface,
    zvariant::{Value,OwnedValue}
};
use std::collections::HashMap;


struct FilePicker;

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FilePicker {
    fn open_file(&self, _ob: zbus::zvariant::ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        eprint!("Open file");
        let multi = options.get("multiple").unwrap_or(&Value::Bool(false));
        let dir = options.get("directory").unwrap_or(&Value::Bool(false));
        let dir = match dir {
            &Value::Bool(true) => 1,
            &Value::Bool(false) => 0,
            _ => { eprintln!("DIR type error"); 0},
        };
        let multi = match multi {
            &Value::Bool(true) => 1,
            &Value::Bool(false) => 0,
            _ => { eprintln!("MULTI type error"); 0},
        };
        let cmd = format!("/usr/local/share/xdg-desktop-portal-pikeru/pikeru-wrapper.sh {} {} 0 /home/d",
                          multi, dir);
        let output = match std::process::Command::new("sh").arg("-c").arg(cmd).output() {
            Ok(out) => unsafe { std::str::from_utf8_unchecked(&out.stdout).to_owned() },
            Err(e) => {eprintln!("Process error: {}", e); "".to_owned()},
        };
        let arr = output.lines().map(|line|{
            Value::from(format!("file://{}",line))
        }).collect::<Vec<_>>();
        let mut ret = HashMap::new();
        let status = if arr.is_empty() { 1 } else {
            let uris = Value::from(arr).try_to_owned().unwrap();
            ret.insert("uris".to_string(), uris);
            0
        };
        eprintln!("RET:({},\n{:#?})", status, ret);
        (status, ret)
    }

    fn save_file(&self, _ob: zbus::zvariant::ObjectPath<'_>, _caller: &str, _parent: &str,
                 _title: &str, options: HashMap<&str, Value<'_>>) -> (u32, HashMap<String, OwnedValue>) {
        eprint!("Save options:{:#?}", options);
        let _fname = match options.get("current_name") {
            Some(val) => val.to_string(),
            None => "download".to_string(),
        };
        (0, HashMap::default())
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
