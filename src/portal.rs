//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use std::{error::Error, future::pending};
use zbus::{connection, interface};
use std::collections::HashMap;


struct FilePicker;

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FilePicker {
    fn open_file(
        &self,
        arg_1: zbus::zvariant::ObjectPath<'_>,
        arg_2: &str,
        arg_3: &str,
        arg_4: &str,
        arg_5: HashMap<&str, zbus::zvariant::Value<'_>>) -> (u32, HashMap<String, zbus::zvariant::OwnedValue>) {
        eprint!("Open file");
        (0, HashMap::default())
    }

    fn save_file(
        &self,
        arg_1: zbus::zvariant::ObjectPath<'_>,
        arg_2: &str,
        arg_3: &str,
        arg_4: &str,
        arg_5: HashMap<&str, zbus::zvariant::Value<'_>>) -> (u32, HashMap<String, zbus::zvariant::OwnedValue>) {
        eprint!("Save file");
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
