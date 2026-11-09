//! Handing files to the user: opening attachments and saving exports.

use std::io;

use crate::model::Id;

/// Desktop: writes the file to a temp folder and opens it with the default app.
#[cfg(not(target_arch = "wasm32"))]
pub fn open_file(attachment: Id, name: &str, bytes: &[u8]) -> io::Result<()> {
    use std::path::Path;
    let dir = std::env::temp_dir()
        .join("fileboxes")
        .join(attachment.to_string());
    std::fs::create_dir_all(&dir)?;
    // Only the last path component, so a stored name cannot point outside `dir`.
    let path = dir.join(Path::new(name).file_name().unwrap_or("file".as_ref()));
    std::fs::write(&path, bytes)?;
    open::that_detached(path)
}

/// Web: downloads the file.
#[cfg(target_arch = "wasm32")]
pub fn open_file(_attachment: Id, name: &str, bytes: &[u8]) -> io::Result<()> {
    download(name, bytes)
}

/// Desktop: asks where to save, then writes the file. Cancelling saves nothing.
#[cfg(not(target_arch = "wasm32"))]
pub async fn save_file(name: &str, bytes: &[u8]) -> io::Result<()> {
    let extension = crate::archive::EXTENSION;
    let picked = rfd::AsyncFileDialog::new()
        .set_file_name(name)
        .add_filter("Filebox", &[extension])
        .save_file()
        .await;
    match picked {
        Some(file) => std::fs::write(file.path(), bytes),
        None => Ok(()),
    }
}

/// Web: downloads the file.
#[cfg(target_arch = "wasm32")]
pub async fn save_file(name: &str, bytes: &[u8]) -> io::Result<()> {
    download(name, bytes)
}

#[cfg(target_arch = "wasm32")]
fn download(name: &str, bytes: &[u8]) -> io::Result<()> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use dioxus::prelude::document;
    let name = serde_json::to_string(name)?;
    document::eval(&format!(
        "const a = document.createElement('a'); \
         a.href = 'data:application/octet-stream;base64,{}'; \
         a.download = {name}; a.click();",
        STANDARD.encode(bytes)
    ));
    Ok(())
}
