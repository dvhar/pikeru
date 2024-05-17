//https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.FileChooser.html
//https://docs.rs/zbus/latest/zbus/index.html
use std::{error::Error, future::pending};
use zbus::{connection, interface};
use zbus::zvariant;

#[derive(zvariant::SerializeDict, zvariant::DeserializeDict, zvariant::Type, Debug)]
#[zvariant(signature = "a{sv}")]
struct FOptions {
    handle_token: String,
    accept_label: String,
    modal: bool,
    multiple: bool,
    directory: bool,
}

#[derive(zvariant::SerializeDict, zvariant::DeserializeDict, zvariant::Type, Debug)]
#[zvariant(signature = "ssa{sv}")]
struct FileRequest {
    parent: String,
    title: String,
    options: FOptions,
}

#[derive(zvariant::SerializeDict, zvariant::DeserializeDict, zvariant::Type, Debug, Default)]
#[zvariant(signature = "(as)")]
struct FileResult {
    uris: Vec<String>,
}

struct FilePicker {
}

#[interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FilePicker {
    async fn open_file(&mut self, args: FileRequest) -> FileResult {
        eprintln!("req: {:?}", args);
        FileResult::default()
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
