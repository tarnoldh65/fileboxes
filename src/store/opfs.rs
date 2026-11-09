//! Origin Private File System storage backend (web). Not implemented until phase 7.

use std::io;

use super::{Store, Summary};
use crate::model::{Filebox, Id};

pub struct OpfsStore;

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "browser storage is not available yet",
    ))
}

impl Store for OpfsStore {
    async fn list(&self) -> io::Result<Vec<Summary>> {
        Ok(Vec::new())
    }

    async fn load(&self, _id: Id) -> io::Result<Filebox> {
        unsupported()
    }

    async fn save(&self, _filebox: &Filebox) -> io::Result<()> {
        unsupported()
    }

    async fn delete(&self, _id: Id) -> io::Result<()> {
        Ok(())
    }

    async fn put_attachment(&self, _filebox: Id, _attachment: Id, _bytes: &[u8]) -> io::Result<()> {
        unsupported()
    }

    async fn get_attachment(&self, _filebox: Id, _attachment: Id) -> io::Result<Vec<u8>> {
        unsupported()
    }
}
