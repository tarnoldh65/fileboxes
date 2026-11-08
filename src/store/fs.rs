//! Filesystem storage backend (desktop).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{ATTACHMENTS_DIR, FILEBOX_FILE, Store, Summary};
use crate::model::{Filebox, Id};

pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    /// Opens a store in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn dir(&self, filebox: Id) -> PathBuf {
        self.root.join(filebox.to_string())
    }

    fn attachments_dir(&self, filebox: Id) -> PathBuf {
        self.dir(filebox).join(ATTACHMENTS_DIR)
    }
}

impl Store for FsStore {
    async fn list(&self) -> io::Result<Vec<Summary>> {
        let mut list = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let path = entry?.path().join(FILEBOX_FILE);
            if path.is_file() {
                list.push(serde_json::from_slice::<Summary>(&fs::read(path)?)?);
            }
        }
        list.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
        Ok(list)
    }

    async fn load(&self, id: Id) -> io::Result<Filebox> {
        let bytes = fs::read(self.dir(id).join(FILEBOX_FILE))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn save(&self, filebox: &Filebox) -> io::Result<()> {
        let dir = self.dir(filebox.id);
        fs::create_dir_all(dir.join(ATTACHMENTS_DIR))?;
        // Write then rename so a crash never leaves a half-written file.
        let tmp = dir.join(format!("{FILEBOX_FILE}.tmp"));
        fs::write(&tmp, serde_json::to_vec_pretty(filebox)?)?;
        fs::rename(tmp, dir.join(FILEBOX_FILE))?;

        let used = filebox.attachments();
        for entry in fs::read_dir(dir.join(ATTACHMENTS_DIR))? {
            let path = entry?.path();
            let id = path.file_name().and_then(|n| n.to_str()?.parse().ok());
            if !id.is_some_and(|id| used.contains(&id)) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    async fn delete(&self, id: Id) -> io::Result<()> {
        fs::remove_dir_all(self.dir(id))
    }

    async fn put_attachment(&self, filebox: Id, bytes: &[u8]) -> io::Result<Id> {
        let dir = self.attachments_dir(filebox);
        fs::create_dir_all(&dir)?;
        let id = Uuid::new_v4();
        fs::write(dir.join(id.to_string()), bytes)?;
        Ok(id)
    }

    async fn get_attachment(&self, filebox: Id, attachment: Id) -> io::Result<Vec<u8>> {
        fs::read(self.attachments_dir(filebox).join(attachment.to_string()))
    }
}

/// Desktop settings: where fileboxes are stored.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub root: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            root: dirs::document_dir()
                .expect("no documents folder")
                .join("Fileboxes"),
        }
    }
}

impl Config {
    /// `<config dir>/fileboxes/config.json`.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .expect("no config folder")
            .join("fileboxes")
            .join("config.json")
    }

    /// Loads the config, or the default if the file is missing or unreadable.
    pub fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path.parent().expect("config path has a parent"))?;
        fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use pollster::block_on;

    use super::*;
    use crate::model::{Block, Content, Folder, Name, Page, Section};

    fn name(s: &str) -> Name {
        Name::new(s).unwrap()
    }

    fn filebox(title: &str) -> Filebox {
        let mut fb = Filebox::new(name(title));
        let mut section = Section::new(name("Section"));
        let mut folder = Folder::new(name("Folder"));
        folder.pages.push(Page::new(name("Page")));
        section.folders.push(folder);
        fb.sections.push(section);
        fb
    }

    fn blocks(fb: &mut Filebox) -> &mut Vec<Block> {
        &mut fb.sections[0].folders[0].pages[0].blocks
    }

    fn image(attachment: Id) -> Block {
        Block::new(Content::Image {
            attachment,
            caption: String::new(),
        })
    }

    #[test]
    fn save_then_load_round_trips_with_expected_layout() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let fb = filebox("Notes");
        block_on(store.save(&fb)).unwrap();

        let fb_dir = dir.path().join(fb.id.to_string());
        assert!(fb_dir.join("filebox.json").is_file());
        assert!(fb_dir.join("attachments").is_dir());
        assert!(!fb_dir.join("filebox.json.tmp").exists());
        assert_eq!(block_on(store.load(fb.id)).unwrap(), fb);
    }

    #[test]
    fn save_overwrites_previous_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let mut fb = filebox("Notes");
        block_on(store.save(&fb)).unwrap();
        fb.name = name("Renamed");
        block_on(store.save(&fb)).unwrap();
        assert_eq!(block_on(store.load(fb.id)).unwrap(), fb);
    }

    #[test]
    fn attachments_round_trip_byte_for_byte() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let mut fb = filebox("Notes");
        let bytes: Vec<u8> = (0..=255).cycle().take(10_000).collect();
        let id = block_on(store.put_attachment(fb.id, &bytes)).unwrap();
        blocks(&mut fb).push(image(id));
        block_on(store.save(&fb)).unwrap();

        let path = dir
            .path()
            .join(fb.id.to_string())
            .join("attachments")
            .join(id.to_string());
        assert!(path.is_file());
        assert_eq!(block_on(store.get_attachment(fb.id, id)).unwrap(), bytes);
    }

    #[test]
    fn save_removes_unreferenced_attachments() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let mut fb = filebox("Notes");
        let kept = block_on(store.put_attachment(fb.id, b"keep")).unwrap();
        let dropped = block_on(store.put_attachment(fb.id, b"drop")).unwrap();
        blocks(&mut fb).extend([image(kept), image(dropped)]);
        block_on(store.save(&fb)).unwrap();

        // Deleting the block frees its attachment on the next save.
        blocks(&mut fb).pop();
        block_on(store.save(&fb)).unwrap();
        assert_eq!(
            block_on(store.get_attachment(fb.id, kept)).unwrap(),
            b"keep"
        );
        assert!(block_on(store.get_attachment(fb.id, dropped)).is_err());
    }

    #[test]
    fn list_returns_every_filebox_sorted_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let (b, a) = (filebox("Beta"), filebox("Alpha"));
        block_on(store.save(&b)).unwrap();
        block_on(store.save(&a)).unwrap();
        fs::create_dir(dir.path().join("not-a-filebox")).unwrap();

        let list = block_on(store.list()).unwrap();
        assert_eq!(
            list,
            [
                Summary {
                    id: a.id,
                    name: a.name
                },
                Summary {
                    id: b.id,
                    name: b.name
                }
            ]
        );
    }

    #[test]
    fn delete_removes_filebox_folder() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let fb = filebox("Notes");
        block_on(store.save(&fb)).unwrap();
        block_on(store.delete(fb.id)).unwrap();
        assert!(!dir.path().join(fb.id.to_string()).exists());
        assert!(block_on(store.list()).unwrap().is_empty());
    }

    #[test]
    fn open_creates_missing_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("a").join("b");
        FsStore::open(&root).unwrap();
        assert!(root.is_dir());
    }

    #[test]
    fn config_round_trips_and_defaults_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fileboxes").join("config.json");
        assert_eq!(Config::load(&path), Config::default());

        let config = Config {
            root: dir.path().join("OneDrive").join("Fileboxes"),
        };
        config.save(&path).unwrap();
        assert_eq!(Config::load(&path), config);
    }
}
