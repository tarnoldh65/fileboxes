//! Storage backends. Every backend uses the same layout:
//! `<root>/<filebox-id>/filebox.json` (structure), `<root>/<filebox-id>/pages/<page-id>.md`
//! (page content), and `<root>/<filebox-id>/attachments/<attachment-id>`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;

use serde::Deserialize;

use crate::model::{Filebox, Id, Name};

#[cfg(not(target_arch = "wasm32"))]
pub mod fs;
#[cfg(target_arch = "wasm32")]
pub mod opfs;

pub const FILEBOX_FILE: &str = "filebox.json";
pub const PAGES_DIR: &str = "pages";
pub const ATTACHMENTS_DIR: &str = "attachments";

pub fn page_file(page: Id) -> String {
    format!("{page}.md")
}

/// The id and name of a stored filebox, read without its contents.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
    /// Saves the filebox, rewriting only pages that changed, and deletes the files of
    /// pages and attachments it no longer has.
    async fn save(&self, filebox: &Filebox) -> io::Result<()>;
    async fn delete(&self, id: Id) -> io::Result<()>;
    /// Stores an attachment under an id chosen by the caller.
    async fn put_attachment(&self, filebox: Id, attachment: Id, bytes: &[u8]) -> io::Result<()>;
    async fn get_attachment(&self, filebox: Id, attachment: Id) -> io::Result<Vec<u8>>;
}

/// The Markdown each page file last held, so saves skip pages that did not change.
#[derive(Default)]
pub struct Written(RefCell<HashMap<(Id, Id), String>>);

impl Written {
    pub fn is_current(&self, filebox: Id, page: Id, md: &str) -> bool {
        self.0
            .borrow()
            .get(&(filebox, page))
            .is_some_and(|m| m == md)
    }

    pub fn set(&self, filebox: Id, page: Id, md: String) {
        self.0.borrow_mut().insert((filebox, page), md);
    }
}
