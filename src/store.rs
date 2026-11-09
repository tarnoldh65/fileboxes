//! Storage backends. Every backend uses the same layout:
//! `<root>/<filebox-id>/filebox.json` and `<root>/<filebox-id>/attachments/<attachment-id>`.

use std::io;

use serde::Deserialize;

use crate::model::{Filebox, Id, Name};

#[cfg(not(target_arch = "wasm32"))]
pub mod fs;
pub mod opfs;

pub const FILEBOX_FILE: &str = "filebox.json";
pub const ATTACHMENTS_DIR: &str = "attachments";

/// The id and name of a stored filebox, read without its contents.
#[derive(Debug, PartialEq, Deserialize)]
pub struct Summary {
    pub id: Id,
    pub name: Name,
}

// Futures are not `Send` so the trait also fits browser storage.
#[allow(async_fn_in_trait)]
pub trait Store {
    /// All stored fileboxes, sorted by name.
    async fn list(&self) -> io::Result<Vec<Summary>>;
    async fn load(&self, id: Id) -> io::Result<Filebox>;
    /// Saves the filebox and deletes its attachments that no block references.
    async fn save(&self, filebox: &Filebox) -> io::Result<()>;
    async fn delete(&self, id: Id) -> io::Result<()>;
    /// Stores a new attachment and returns its id.
    async fn put_attachment(&self, filebox: Id, bytes: &[u8]) -> io::Result<Id>;
    async fn get_attachment(&self, filebox: Id, attachment: Id) -> io::Result<Vec<u8>>;
}
