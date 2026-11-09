//! Origin Private File System storage backend (web).

use std::io;

use js_sys::{ArrayBuffer, Promise, Reflect, Uint8Array};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    File, FileSystemDirectoryHandle as Dir, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemRemoveOptions, FileSystemWritableFileStream,
};

use super::{ATTACHMENTS_DIR, FILEBOX_FILE, Store, Summary};
use crate::model::{Filebox, Id};

/// Stores fileboxes in a folder of the browser's private storage for this site.
pub struct OpfsStore {
    root: String,
}

impl OpfsStore {
    /// Uses the folder `root`, created on first use. Also asks the browser to keep the
    /// data instead of evicting it under storage pressure.
    pub fn new(root: impl Into<String>) -> Self {
        _ = storage().persist();
        Self { root: root.into() }
    }

    async fn root(&self) -> io::Result<Dir> {
        let opfs: Dir = resolve(storage().get_directory()).await?;
        dir(&opfs, &self.root, true).await
    }

    async fn filebox_dir(&self, id: Id, create: bool) -> io::Result<Dir> {
        dir(&self.root().await?, &id.to_string(), create).await
    }

    async fn attachments_dir(&self, id: Id, create: bool) -> io::Result<Dir> {
        dir(
            &self.filebox_dir(id, create).await?,
            ATTACHMENTS_DIR,
            create,
        )
        .await
    }
}

fn storage() -> web_sys::StorageManager {
    web_sys::window()
        .expect("running in a browser window")
        .navigator()
        .storage()
}

fn js_error(e: JsValue) -> io::Error {
    let message = match e.dyn_ref::<js_sys::Error>() {
        Some(e) => String::from(e.message()),
        None => format!("{e:?}"),
    };
    io::Error::other(message)
}

async fn resolve<T: JsCast>(promise: Promise) -> io::Result<T> {
    Ok(JsFuture::from(promise)
        .await
        .map_err(js_error)?
        .unchecked_into())
}

async fn dir(parent: &Dir, name: &str, create: bool) -> io::Result<Dir> {
    let options = FileSystemGetDirectoryOptions::new();
    options.set_create(create);
    resolve(parent.get_directory_handle_with_options(name, &options)).await
}

async fn read(dir: &Dir, name: &str) -> io::Result<Vec<u8>> {
    let handle: FileSystemFileHandle = resolve(dir.get_file_handle(name)).await?;
    let file: File = resolve(handle.get_file()).await?;
    let buffer: ArrayBuffer = resolve(file.array_buffer()).await?;
    Ok(Uint8Array::new(&buffer).to_vec())
}

/// Writes a whole file. The browser commits it on close, so a failed write never
/// leaves a half-written file.
async fn write(dir: &Dir, name: &str, bytes: &[u8]) -> io::Result<()> {
    let options = FileSystemGetFileOptions::new();
    options.set_create(true);
    let handle: FileSystemFileHandle =
        resolve(dir.get_file_handle_with_options(name, &options)).await?;
    let stream: FileSystemWritableFileStream = resolve(handle.create_writable()).await?;
    resolve::<JsValue>(stream.write_with_u8_array(bytes).map_err(js_error)?).await?;
    resolve::<JsValue>(stream.close()).await?;
    Ok(())
}

/// Names of everything in a folder.
async fn names(dir: &Dir) -> io::Result<Vec<String>> {
    let keys = dir.keys();
    let mut names = Vec::new();
    loop {
        let next: JsValue = resolve(keys.next().map_err(js_error)?).await?;
        if Reflect::get(&next, &"done".into())
            .map_err(js_error)?
            .is_truthy()
        {
            return Ok(names);
        }
        let name = Reflect::get(&next, &"value".into()).map_err(js_error)?;
        names.push(name.as_string().expect("entry names are strings"));
    }
}

impl Store for OpfsStore {
    async fn list(&self) -> io::Result<Vec<Summary>> {
        let root = self.root().await?;
        let mut list = Vec::new();
        for name in names(&root).await? {
            // Skip anything that is not a folder holding a filebox.
            let Ok(dir) = dir(&root, &name, false).await else {
                continue;
            };
            if let Ok(bytes) = read(&dir, FILEBOX_FILE).await {
                list.push(serde_json::from_slice::<Summary>(&bytes)?);
            }
        }
        list.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        Ok(list)
    }

    async fn load(&self, id: Id) -> io::Result<Filebox> {
        let bytes = read(&self.filebox_dir(id, false).await?, FILEBOX_FILE).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn save(&self, filebox: &Filebox) -> io::Result<()> {
        let dir = self.filebox_dir(filebox.id, true).await?;
        write(&dir, FILEBOX_FILE, &serde_json::to_vec_pretty(filebox)?).await?;

        let attachments = self.attachments_dir(filebox.id, true).await?;
        let used = filebox.attachments();
        for name in names(&attachments).await? {
            if !name.parse().is_ok_and(|id| used.contains(&id)) {
                resolve::<JsValue>(attachments.remove_entry(&name)).await?;
            }
        }
        Ok(())
    }

    async fn delete(&self, id: Id) -> io::Result<()> {
        let options = FileSystemRemoveOptions::new();
        options.set_recursive(true);
        let root = self.root().await?;
        resolve::<JsValue>(root.remove_entry_with_options(&id.to_string(), &options)).await?;
        Ok(())
    }

    async fn put_attachment(&self, filebox: Id, attachment: Id, bytes: &[u8]) -> io::Result<()> {
        let dir = self.attachments_dir(filebox, true).await?;
        write(&dir, &attachment.to_string(), bytes).await
    }

    async fn get_attachment(&self, filebox: Id, attachment: Id) -> io::Result<Vec<u8>> {
        let dir = self.attachments_dir(filebox, false).await?;
        read(&dir, &attachment.to_string()).await
    }
}
