//! Filesystem storage backend (desktop).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{ATTACHMENTS_DIR, FILEBOX_FILE, PAGES_DIR, Store, Summary, Written, page_file};
use crate::markdown::{from_markdown, to_markdown};
use crate::model::{Filebox, Id};

pub struct FsStore {
    root: PathBuf,
    written: Written,
}

impl FsStore {
    /// Opens a store in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            written: Written::default(),
        })
    }

    fn dir(&self, filebox: Id) -> PathBuf {
        self.root.join(filebox.to_string())
    }

    fn attachments_dir(&self, filebox: Id) -> PathBuf {
        self.dir(filebox).join(ATTACHMENTS_DIR)
    }
}

/// Writes then renames, so a crash never leaves a half-written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// Deletes every file in `dir` whose name is not in `keep`.
fn remove_others(dir: &Path, keep: &HashSet<String>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !keep.contains(name) {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
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
        let dir = self.dir(id);
        let mut filebox: Filebox = serde_json::from_slice(&fs::read(dir.join(FILEBOX_FILE))?)?;
        for page in filebox.pages_mut() {
            // Pages without a file are from before Markdown storage and keep their blocks.
            if let Ok(md) = fs::read_to_string(dir.join(PAGES_DIR).join(page_file(page.id))) {
                page.blocks = from_markdown(&md);
                self.written.set(id, page.id, md);
            }
        }
        Ok(filebox)
    }

    async fn save(&self, filebox: &Filebox) -> io::Result<()> {
        let dir = self.dir(filebox.id);
        let pages = dir.join(PAGES_DIR);
        fs::create_dir_all(&pages)?;
        fs::create_dir_all(dir.join(ATTACHMENTS_DIR))?;
        write_atomic(
            &dir.join(FILEBOX_FILE),
            &serde_json::to_vec_pretty(filebox)?,
        )?;

        for page in filebox.pages() {
            let md = to_markdown(&page.blocks);
            if !self.written.is_current(filebox.id, page.id, &md) {
                write_atomic(&pages.join(page_file(page.id)), md.as_bytes())?;
                self.written.set(filebox.id, page.id, md);
            }
        }
        remove_others(&pages, &filebox.pages().map(|p| page_file(p.id)).collect())?;
        let used = filebox.attachments().iter().map(Id::to_string).collect();
        remove_others(&dir.join(ATTACHMENTS_DIR), &used)
    }

    async fn delete(&self, id: Id) -> io::Result<()> {
        fs::remove_dir_all(self.dir(id))
    }

    async fn put_attachment(&self, filebox: Id, attachment: Id, bytes: &[u8]) -> io::Result<()> {
        let dir = self.attachments_dir(filebox);
        fs::create_dir_all(&dir)?;
        fs::write(dir.join(attachment.to_string()), bytes)
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
    /// `Documents/Fileboxes`, or `~/Fileboxes` where there is no Documents folder
    /// (common on Linux).
    fn default() -> Self {
        let parent = dirs::document_dir()
            .or_else(dirs::home_dir)
            .expect("no home folder");
        Self {
            root: parent.join("Fileboxes"),
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

/// Backend-independent storage tests live in `tests/suite`.
#[cfg(test)]
mod tests {
    use pollster::block_on;

    use super::*;
    use crate::model::{Block, Content, Folder, Name, Page, Section};

    fn filebox(title: &str) -> Filebox {
        Filebox::new(Name::new(title).unwrap())
    }

    /// A filebox with one page holding one bold paragraph.
    fn with_page() -> Filebox {
        let mut fb = filebox("Notes");
        let mut section = Section::new(Name::new("Section").unwrap());
        let mut folder = Folder::new(Name::new("Folder").unwrap());
        let mut page = Page::new(Name::new("Page").unwrap());
        page.blocks.push(Block::new(Content::Text {
            html: "<p><strong>Hi</strong></p>".into(),
        }));
        folder.pages.push(page);
        section.folders.push(folder);
        fb.sections.push(section);
        fb
    }

    fn page_path(dir: &Path, fb: &Filebox) -> PathBuf {
        let page = fb.pages().next().unwrap().id;
        dir.join(fb.id.to_string())
            .join("pages")
            .join(format!("{page}.md"))
    }

    #[test]
    fn save_uses_expected_layout() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let fb = with_page();
        block_on(store.save(&fb)).unwrap();

        let fb_dir = dir.path().join(fb.id.to_string());
        assert!(fb_dir.join("filebox.json").is_file());
        assert!(fb_dir.join("attachments").is_dir());
        assert!(!fb_dir.join("filebox.json.tmp").exists());
        let json = fs::read_to_string(fb_dir.join("filebox.json")).unwrap();
        assert!(!json.contains("blocks") && !json.contains("Hi"));
        assert_eq!(
            fs::read_to_string(page_path(dir.path(), &fb)).unwrap(),
            "<!-- block -->
**Hi**

"
        );
    }

    #[test]
    fn save_rewrites_only_changed_pages() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let mut fb = with_page();
        block_on(store.save(&fb)).unwrap();

        // A file left alone by an unchanged save keeps whatever is in it.
        let path = page_path(dir.path(), &fb);
        fs::write(&path, "untouched").unwrap();
        block_on(store.save(&fb)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "untouched");

        fb.pages_mut().next().unwrap().blocks.clear();
        block_on(store.save(&fb)).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
    }

    #[test]
    fn deleting_a_page_removes_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let mut fb = with_page();
        block_on(store.save(&fb)).unwrap();
        let path = page_path(dir.path(), &fb);
        assert!(path.is_file());

        fb.sections[0].folders[0].pages.clear();
        block_on(store.save(&fb)).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn old_filebox_with_blocks_in_json_loads_and_migrates() {
        let dir = tempfile::tempdir().unwrap();
        let fb = with_page();
        let mut json = serde_json::to_value(&fb).unwrap();
        json["sections"][0]["folders"][0]["pages"][0]["blocks"] =
            serde_json::to_value(&fb.sections[0].folders[0].pages[0].blocks).unwrap();
        let fb_dir = dir.path().join(fb.id.to_string());
        fs::create_dir_all(&fb_dir).unwrap();
        fs::write(fb_dir.join("filebox.json"), json.to_string()).unwrap();

        let store = FsStore::open(dir.path()).unwrap();
        let loaded = block_on(store.load(fb.id)).unwrap();
        assert_eq!(loaded, fb);
        block_on(store.save(&loaded)).unwrap();
        assert!(page_path(dir.path(), &fb).is_file());
        let json = fs::read_to_string(fb_dir.join("filebox.json")).unwrap();
        assert!(!json.contains("blocks"));
        let reopened = FsStore::open(dir.path()).unwrap();
        assert_eq!(block_on(reopened.load(fb.id)).unwrap(), fb);
    }

    #[test]
    fn list_ignores_folders_that_are_not_fileboxes() {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        let fb = filebox("Notes");
        block_on(store.save(&fb)).unwrap();
        fs::create_dir(dir.path().join("not-a-filebox")).unwrap();
        fs::write(dir.path().join("stray.txt"), "x").unwrap();
        let list = block_on(store.list()).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, fb.id);
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
