//! App state shared by all components, and the actions that change it.

use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::time::Duration;

use dioxus::prelude::*;
use futures_timer::Delay;

use crate::model::{Filebox, Folder, Id, Item, Name, Page, Section, find_mut};
use crate::store::{Store, Summary};

#[cfg(not(target_arch = "wasm32"))]
pub type AppStore = crate::store::fs::FsStore;
#[cfg(target_arch = "wasm32")]
pub type AppStore = crate::store::opfs::OpfsStore;

#[cfg(not(target_arch = "wasm32"))]
pub fn open_store() -> AppStore {
    use crate::store::fs::{Config, FsStore};
    let root = Config::load(&Config::default_path()).root;
    FsStore::open(&root).unwrap_or_else(|e| panic!("cannot open {}: {e}", root.display()))
}

#[cfg(target_arch = "wasm32")]
pub fn open_store() -> AppStore {
    crate::store::opfs::OpfsStore
}

/// What a rename, menu, or delete applies to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Target {
    Filebox,
    Item(Item),
}

/// The selected section, folder, and page of one filebox. Ids may be stale; see [`resolve`].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Selection {
    pub section: Option<Id>,
    pub folder: Option<Id>,
    pub page: Option<Id>,
}

/// The selected items that exist, falling back to the first item at each level.
pub fn resolve<'a>(
    fb: &'a Filebox,
    sel: &Selection,
) -> (Option<&'a Section>, Option<&'a Folder>, Option<&'a Page>) {
    fn pick<T>(items: &[T], id: Option<Id>, id_of: impl Fn(&T) -> Id) -> Option<&T> {
        id.and_then(|id| items.iter().find(|i| id_of(i) == id))
            .or(items.first())
    }
    let section = pick(&fb.sections, sel.section, |s| s.id);
    let folder = section.and_then(|s| pick(&s.folders, sel.folder, |f| f.id));
    let page = folder.and_then(|f| pick(&f.pages, sel.page, |p| p.id));
    (section, folder, page)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Menu {
    pub target: Target,
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Copy)]
pub struct AppState {
    pub store: Signal<Rc<AppStore>>,
    pub boxes: Signal<Vec<Summary>>,
    pub filebox: Signal<Option<Filebox>>,
    pub selections: Signal<HashMap<Id, Selection>>,
    pub renaming: Signal<Option<Target>>,
    pub menu: Signal<Option<Menu>>,
    pub deleting: Signal<Option<Target>>,
    pub error: Signal<Option<String>>,
}

fn name(s: &str) -> Name {
    Name::new(s).expect("default names are not empty")
}

impl AppState {
    pub fn new(store: AppStore) -> Self {
        Self {
            store: Signal::new(Rc::new(store)),
            boxes: Signal::new(Vec::new()),
            filebox: Signal::new(None),
            selections: Signal::new(HashMap::new()),
            renaming: Signal::new(None),
            menu: Signal::new(None),
            deleting: Signal::new(None),
            error: Signal::new(None),
        }
    }

    /// The current filebox's selection (subscribes the caller).
    pub fn selection(&self) -> Selection {
        let id = self.filebox.read().as_ref().map(|fb| fb.id);
        id.and_then(|id| self.selections.read().get(&id).copied())
            .unwrap_or_default()
    }

    /// Ids of the resolved selection, read without subscribing.
    fn current(&self) -> Selection {
        let fb = self.filebox.peek();
        let Some(fb) = fb.as_ref() else {
            return Selection::default();
        };
        let sel = self
            .selections
            .peek()
            .get(&fb.id)
            .copied()
            .unwrap_or_default();
        let (s, f, p) = resolve(fb, &sel);
        Selection {
            section: s.map(|s| s.id),
            folder: f.map(|f| f.id),
            page: p.map(|p| p.id),
        }
    }

    pub fn select(&mut self, change: impl FnOnce(&mut Selection)) {
        let Some(id) = self.filebox.peek().as_ref().map(|fb| fb.id) else {
            return;
        };
        let current = self.current();
        let mut selections = self.selections.write();
        let sel = selections.entry(id).or_default();
        *sel = current;
        change(sel);
    }

    fn edit(&mut self, change: impl FnOnce(&mut Filebox)) {
        if let Some(fb) = self.filebox.write().as_mut() {
            change(fb);
        }
    }

    fn report<T>(&mut self, result: io::Result<T>) -> Option<T> {
        result.map_err(|e| self.error.set(Some(e.to_string()))).ok()
    }

    /// Loads the filebox list and opens the first filebox.
    pub async fn start(self) {
        self.refresh().await;
        let first = self.boxes.peek().first().map(|b| b.id);
        if let Some(id) = first {
            self.open(id).await;
        }
    }

    async fn refresh(mut self) {
        let store = self.store.peek().clone();
        if let Some(boxes) = self.report(store.list().await) {
            self.boxes.set(boxes);
        }
    }

    /// Saves the current filebox now instead of waiting for autosave.
    async fn flush(mut self) {
        let Some(fb) = self.filebox.peek().clone() else {
            return;
        };
        let store = self.store.peek().clone();
        self.report(store.save(&fb).await);
    }

    pub async fn open(mut self, id: Id) {
        if self.filebox.peek().as_ref().is_some_and(|fb| fb.id == id) {
            return;
        }
        self.flush().await;
        let store = self.store.peek().clone();
        if let Some(fb) = self.report(store.load(id).await) {
            self.renaming.set(None);
            self.filebox.set(Some(fb));
        }
    }

    pub async fn new_filebox(mut self) {
        self.flush().await;
        let fb = Filebox::new(name("New filebox"));
        let store = self.store.peek().clone();
        self.report(store.save(&fb).await);
        self.filebox.set(Some(fb));
        self.refresh().await;
        self.renaming.set(Some(Target::Filebox));
    }

    async fn delete_filebox(mut self) {
        // Clearing the filebox first also cancels any pending autosave of it.
        let Some(fb) = self.filebox.take() else {
            return;
        };
        let store = self.store.peek().clone();
        self.report(store.delete(fb.id).await);
        self.selections.write().remove(&fb.id);
        self.start().await;
    }

    /// Desktop only: lets the user pick the storage folder and remembers it.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn choose_folder(mut self) {
        use crate::store::fs::{Config, FsStore};
        let Some(dir) = rfd::AsyncFileDialog::new().pick_folder().await else {
            return;
        };
        let config = Config {
            root: dir.path().to_path_buf(),
        };
        let Some(store) = self.report(FsStore::open(&config.root)) else {
            return;
        };
        self.flush().await;
        self.report(config.save(&Config::default_path()));
        self.filebox.set(None);
        self.store.set(Rc::new(store));
        self.start().await;
    }

    /// Applies a rename typed by the user; empty names are ignored.
    pub fn rename(&mut self, target: Target, value: &str) {
        if *self.renaming.peek() != Some(target) {
            return;
        }
        self.renaming.set(None);
        let Ok(new) = Name::new(value) else {
            return;
        };
        match target {
            Target::Filebox => {
                let mut id = None;
                self.edit(|fb| {
                    fb.name = new.clone();
                    id = Some(fb.id);
                });
                let mut boxes = self.boxes.write();
                if let Some(b) = boxes.iter_mut().find(|b| Some(b.id) == id) {
                    b.name = new;
                }
                boxes.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
            }
            Target::Item(item) => self.edit(|fb| _ = fb.rename(item, new)),
        }
    }

    pub fn delete(&mut self, target: Target) {
        self.deleting.set(None);
        match target {
            Target::Filebox => _ = spawn(self.delete_filebox()),
            Target::Item(item) => self.edit(|fb| _ = fb.delete(item)),
        }
    }

    pub fn add_section(&mut self) {
        let section = Section::new(name("New section"));
        let id = section.id;
        self.edit(|fb| fb.sections.push(section));
        self.select(|s| s.section = Some(id));
        self.renaming.set(Some(Target::Item(Item::Section(id))));
    }

    pub fn add_folder(&mut self) {
        let Some(section) = self.current().section else {
            return;
        };
        let folder = Folder::new(name("New folder"));
        let id = folder.id;
        self.edit(|fb| {
            find_mut(&mut fb.sections, section)
                .expect("current section exists")
                .folders
                .push(folder)
        });
        self.select(|s| s.folder = Some(id));
        self.renaming.set(Some(Target::Item(Item::Folder(id))));
    }

    pub fn add_page(&mut self) {
        let Selection {
            section: Some(section),
            folder: Some(folder),
            ..
        } = self.current()
        else {
            return;
        };
        let page = Page::new(name("New page"));
        let id = page.id;
        self.edit(|fb| {
            let section = find_mut(&mut fb.sections, section).expect("current section exists");
            find_mut(&mut section.folders, folder)
                .expect("current folder exists")
                .pages
                .push(page)
        });
        self.select(|s| s.page = Some(id));
        self.renaming.set(Some(Target::Item(Item::Page(id))));
    }
}

/// Saves the filebox 1 second after the last change. A newer change restarts the wait.
pub fn use_autosave(state: AppState) {
    use_resource(move || {
        let fb = state.filebox.cloned();
        async move {
            let Some(fb) = fb else {
                return;
            };
            Delay::new(Duration::from_secs(1)).await;
            let store = state.store.peek().clone();
            let mut state = state;
            state.report(store.save(&fb).await);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Filebox {
        let mut fb = Filebox::new(name("Box"));
        for s in ["S1", "S2"] {
            let mut section = Section::new(name(s));
            for f in ["F1", "F2"] {
                let mut folder = Folder::new(name(&format!("{s}{f}")));
                folder.pages.push(Page::new(name(&format!("{s}{f}P1"))));
                folder.pages.push(Page::new(name(&format!("{s}{f}P2"))));
                section.folders.push(folder);
            }
            fb.sections.push(section);
        }
        fb
    }

    fn names(fb: &Filebox, sel: &Selection) -> (String, String, String) {
        let (s, f, p) = resolve(fb, sel);
        (
            s.unwrap().name.to_string(),
            f.unwrap().name.to_string(),
            p.unwrap().title.to_string(),
        )
    }

    #[test]
    fn resolve_defaults_to_first_items() {
        let fb = sample();
        let expected = ("S1".into(), "S1F1".into(), "S1F1P1".into());
        assert_eq!(names(&fb, &Selection::default()), expected);
    }

    #[test]
    fn resolve_keeps_valid_selection() {
        let fb = sample();
        let sel = Selection {
            section: Some(fb.sections[1].id),
            folder: Some(fb.sections[1].folders[1].id),
            page: Some(fb.sections[1].folders[1].pages[1].id),
        };
        let expected = ("S2".into(), "S2F2".into(), "S2F2P2".into());
        assert_eq!(names(&fb, &sel), expected);
    }

    #[test]
    fn resolve_ignores_folder_from_another_section() {
        let fb = sample();
        let sel = Selection {
            section: Some(fb.sections[1].id),
            folder: Some(fb.sections[0].folders[1].id),
            page: None,
        };
        let expected = ("S2".into(), "S2F1".into(), "S2F1P1".into());
        assert_eq!(names(&fb, &sel), expected);
    }

    #[test]
    fn resolve_empty_levels_are_none() {
        let mut fb = sample();
        fb.sections[0].folders.clear();
        assert!(matches!(
            resolve(&fb, &Selection::default()),
            (Some(_), None, None)
        ));
        fb.sections.clear();
        assert!(matches!(
            resolve(&fb, &Selection::default()),
            (None, None, None)
        ));
    }
}
