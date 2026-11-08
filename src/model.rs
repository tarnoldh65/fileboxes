//! Fileboxes data model: a filebox holds sections, which hold folders, which hold pages.

use std::collections::HashSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Id = Uuid;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    EmptyName,
    NotFound,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Error::EmptyName => "name cannot be empty",
            Error::NotFound => "item not found",
        })
    }
}

/// A non-empty, trimmed name or title.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Result<Self, Error> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::EmptyName);
        }
        Ok(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Name {
    type Error = Error;

    fn try_from(name: String) -> Result<Self, Error> {
        Self::new(&name)
    }
}

impl From<Name> for String {
    fn from(name: Name) -> Self {
        name.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filebox {
    pub id: Id,
    pub name: Name,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub id: Id,
    pub name: Name,
    pub folders: Vec<Folder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    pub id: Id,
    pub name: Name,
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: Id,
    pub title: Name,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: Id,
    pub half_width: bool,
    pub content: Content,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    Text { html: String },
    Table { rows: Vec<Vec<String>> },
    Image { attachment: Id, caption: String },
    File { attachment: Id, file_name: String },
    Code { text: String },
}

impl Filebox {
    pub fn new(name: Name) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            sections: Vec::new(),
        }
    }

    /// Moves a page from any folder to the end of `to_folder`.
    pub fn move_page(&mut self, page: Id, to_folder: Id) -> Result<(), Error> {
        self.folder_mut(to_folder)?;
        let page = self
            .folders_mut()
            .find_map(|f| remove(&mut f.pages, page).ok())
            .ok_or(Error::NotFound)?;
        self.folder_mut(to_folder)?.pages.push(page);
        Ok(())
    }

    /// Moves a folder from any section to the end of `to_section`.
    pub fn move_folder(&mut self, folder: Id, to_section: Id) -> Result<(), Error> {
        find_mut(&mut self.sections, to_section)?;
        let folder = self
            .sections
            .iter_mut()
            .find_map(|s| remove(&mut s.folders, folder).ok())
            .ok_or(Error::NotFound)?;
        find_mut(&mut self.sections, to_section)?
            .folders
            .push(folder);
        Ok(())
    }

    /// Ids of every attachment referenced by an image or file block.
    pub fn attachments(&self) -> HashSet<Id> {
        self.sections
            .iter()
            .flat_map(|s| &s.folders)
            .flat_map(|f| &f.pages)
            .flat_map(|p| &p.blocks)
            .filter_map(|b| match b.content {
                Content::Image { attachment, .. } | Content::File { attachment, .. } => {
                    Some(attachment)
                }
                _ => None,
            })
            .collect()
    }

    fn folders_mut(&mut self) -> impl Iterator<Item = &mut Folder> {
        self.sections.iter_mut().flat_map(|s| &mut s.folders)
    }

    fn folder_mut(&mut self, id: Id) -> Result<&mut Folder, Error> {
        self.folders_mut()
            .find(|f| f.id == id)
            .ok_or(Error::NotFound)
    }
}

impl Section {
    pub fn new(name: Name) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            folders: Vec::new(),
        }
    }
}

impl Folder {
    pub fn new(name: Name) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            pages: Vec::new(),
        }
    }
}

impl Page {
    pub fn new(title: Name) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            blocks: Vec::new(),
        }
    }
}

impl Block {
    pub fn new(content: Content) -> Self {
        Self {
            id: Uuid::new_v4(),
            half_width: false,
            content,
        }
    }
}

pub trait HasId {
    fn id(&self) -> Id;
}

macro_rules! has_id {
    ($($t:ty),*) => {
        $(impl HasId for $t {
            fn id(&self) -> Id {
                self.id
            }
        })*
    };
}

has_id!(Section, Folder, Page, Block);

pub fn find_mut<T: HasId>(items: &mut [T], id: Id) -> Result<&mut T, Error> {
    items
        .iter_mut()
        .find(|i| i.id() == id)
        .ok_or(Error::NotFound)
}

/// Removes and returns an item; for sections and folders this drops everything inside.
pub fn remove<T: HasId>(items: &mut Vec<T>, id: Id) -> Result<T, Error> {
    let index = items
        .iter()
        .position(|i| i.id() == id)
        .ok_or(Error::NotFound)?;
    Ok(items.remove(index))
}

/// Moves an item to position `to`. Panics if `to` is not a valid index.
pub fn reorder<T: HasId>(items: &mut Vec<T>, id: Id, to: usize) -> Result<(), Error> {
    let item = remove(items, id)?;
    items.insert(to, item);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> Name {
        Name::new(s).unwrap()
    }

    /// Two sections, each with two folders, each with two pages.
    fn sample() -> Filebox {
        let mut fb = Filebox::new(name("Box"));
        for s in ["S1", "S2"] {
            let mut section = Section::new(name(s));
            for f in ["F1", "F2"] {
                let mut folder = Folder::new(name(&format!("{s}{f}")));
                for p in ["P1", "P2"] {
                    folder.pages.push(Page::new(name(&format!("{s}{f}{p}"))));
                }
                section.folders.push(folder);
            }
            fb.sections.push(section);
        }
        fb
    }

    fn page_titles(folder: &Folder) -> Vec<&str> {
        folder.pages.iter().map(|p| p.title.as_str()).collect()
    }

    #[test]
    fn empty_names_are_rejected() {
        assert_eq!(Name::new(""), Err(Error::EmptyName));
        assert_eq!(Name::new("  \t"), Err(Error::EmptyName));
        assert_eq!(Name::new("  Notes ").unwrap().as_str(), "Notes");
    }

    #[test]
    fn empty_name_is_rejected_when_loading_json() {
        let json = r#"{"id":"00000000-0000-0000-0000-000000000000","name":" ","sections":[]}"#;
        assert!(serde_json::from_str::<Filebox>(json).is_err());
    }

    #[test]
    fn rename_changes_only_target() {
        let mut fb = sample();
        let expected_other = fb.sections[1].clone();
        let id = fb.sections[0].folders[1].id;
        find_mut(&mut fb.sections[0].folders, id).unwrap().name = name("Renamed");
        assert_eq!(fb.sections[0].folders[0].name.as_str(), "S1F1");
        assert_eq!(fb.sections[0].folders[1].name.as_str(), "Renamed");
        assert_eq!(fb.sections[1], expected_other);
    }

    #[test]
    fn remove_page_keeps_siblings_in_order() {
        let mut fb = sample();
        let folder = &mut fb.sections[0].folders[0];
        folder.pages.push(Page::new(name("S1F1P3")));
        let id = folder.pages[1].id;
        let removed = remove(&mut folder.pages, id).unwrap();
        assert_eq!(removed.title.as_str(), "S1F1P2");
        assert_eq!(page_titles(folder), ["S1F1P1", "S1F1P3"]);
    }

    #[test]
    fn remove_section_drops_its_contents_only() {
        let mut fb = sample();
        let kept = fb.sections[1].clone();
        let id = fb.sections[0].id;
        remove(&mut fb.sections, id).unwrap();
        assert_eq!(fb.sections, [kept]);
    }

    #[test]
    fn missing_ids_are_not_found() {
        let mut fb = sample();
        let before = fb.clone();
        let missing = Uuid::new_v4();
        assert_eq!(remove(&mut fb.sections, missing), Err(Error::NotFound));
        assert_eq!(reorder(&mut fb.sections, missing, 0), Err(Error::NotFound));
        assert_eq!(find_mut(&mut fb.sections, missing), Err(Error::NotFound));
        assert_eq!(fb, before);
    }

    #[test]
    fn reorder_moves_item_and_keeps_others_in_order() {
        let mut fb = sample();
        let folder = &mut fb.sections[0].folders[0];
        folder.pages.push(Page::new(name("S1F1P3")));
        let id = folder.pages[2].id;
        reorder(&mut folder.pages, id, 0).unwrap();
        assert_eq!(page_titles(folder), ["S1F1P3", "S1F1P1", "S1F1P2"]);
        reorder(&mut folder.pages, id, 2).unwrap();
        assert_eq!(page_titles(folder), ["S1F1P1", "S1F1P2", "S1F1P3"]);
    }

    #[test]
    fn move_page_to_another_section_folder() {
        let mut fb = sample();
        let page = fb.sections[0].folders[0].pages[0].clone();
        let target = fb.sections[1].folders[1].id;
        fb.move_page(page.id, target).unwrap();
        assert_eq!(page_titles(&fb.sections[0].folders[0]), ["S1F1P2"]);
        assert_eq!(
            page_titles(&fb.sections[1].folders[1]),
            ["S2F2P1", "S2F2P2", "S1F1P1"]
        );
        assert_eq!(fb.sections[1].folders[1].pages[2], page);
    }

    #[test]
    fn move_page_to_missing_folder_changes_nothing() {
        let mut fb = sample();
        let before = fb.clone();
        let page = fb.sections[0].folders[0].pages[0].id;
        assert_eq!(fb.move_page(page, Uuid::new_v4()), Err(Error::NotFound));
        assert_eq!(fb, before);
    }

    #[test]
    fn move_folder_to_another_section() {
        let mut fb = sample();
        let folder = fb.sections[0].folders[1].clone();
        let target = fb.sections[1].id;
        fb.move_folder(folder.id, target).unwrap();
        assert_eq!(fb.sections[0].folders.len(), 1);
        assert_eq!(fb.sections[1].folders.len(), 3);
        assert_eq!(fb.sections[1].folders[2], folder);
    }

    #[test]
    fn move_folder_to_missing_section_changes_nothing() {
        let mut fb = sample();
        let before = fb.clone();
        let folder = fb.sections[0].folders[0].id;
        assert_eq!(fb.move_folder(folder, Uuid::new_v4()), Err(Error::NotFound));
        assert_eq!(fb, before);
    }

    #[test]
    fn ids_are_unique() {
        let fb = sample();
        let mut ids = HashSet::from([fb.id]);
        for s in &fb.sections {
            assert!(ids.insert(s.id));
            for f in &s.folders {
                assert!(ids.insert(f.id));
                for p in &f.pages {
                    assert!(ids.insert(p.id));
                }
            }
        }
        assert_eq!(ids.len(), 1 + 2 + 4 + 8);
    }

    #[test]
    fn json_round_trip_with_every_block_type() {
        let mut fb = sample();
        let mut half = Block::new(Content::Code {
            text: "fn main() {\n    println!(\"hi\");\n}".into(),
        });
        half.half_width = true;
        fb.sections[0].folders[0].pages[0].blocks = vec![
            Block::new(Content::Text {
                html: "<p><b>Bold</b> and <i>italic</i></p>".into(),
            }),
            Block::new(Content::Table {
                rows: vec![vec!["a".into(), "b".into()], vec!["c".into(), "d".into()]],
            }),
            Block::new(Content::Image {
                attachment: Uuid::new_v4(),
                caption: "A picture".into(),
            }),
            Block::new(Content::File {
                attachment: Uuid::new_v4(),
                file_name: "report.pdf".into(),
            }),
            half,
        ];
        let json = serde_json::to_string(&fb).unwrap();
        assert_eq!(serde_json::from_str::<Filebox>(&json).unwrap(), fb);
    }

    #[test]
    fn attachments_collects_image_and_file_blocks_on_every_page() {
        let mut fb = sample();
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        fb.sections[0].folders[0].pages[0].blocks = vec![
            Block::new(Content::Image {
                attachment: a,
                caption: String::new(),
            }),
            Block::new(Content::Code {
                text: String::new(),
            }),
        ];
        fb.sections[1].folders[1].pages[1].blocks = vec![Block::new(Content::File {
            attachment: b,
            file_name: "x.txt".into(),
        })];
        assert_eq!(fb.attachments(), HashSet::from([a, b]));
    }
}
