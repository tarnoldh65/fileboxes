//! Export and import of `.filebox` zip archives: `filebox.json` plus
//! `attachments/<attachment-id>`, the same layout as a store folder.

use std::io::{self, Cursor, Read, Write};

use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::model::{Filebox, Id};
use crate::store::{ATTACHMENTS_DIR, FILEBOX_FILE, Store};

pub const EXTENSION: &str = "filebox";

/// Zips a stored filebox and the attachments it references.
pub async fn export(store: &impl Store, id: Id) -> io::Result<Vec<u8>> {
    let filebox = store.load(id).await?;
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(FILEBOX_FILE, options)?;
    zip.write_all(&serde_json::to_vec_pretty(&filebox)?)?;
    for attachment in filebox.attachments() {
        zip.start_file(format!("{ATTACHMENTS_DIR}/{attachment}"), options)?;
        zip.write_all(&store.get_attachment(id, attachment).await?)?;
    }
    Ok(zip.finish()?.into_inner())
}

/// Unzips a filebox into the store and returns it. If a filebox with the same id is
/// already stored, the import becomes a copy with a new id.
pub async fn import(store: &impl Store, bytes: &[u8]) -> io::Result<Filebox> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut filebox: Filebox = serde_json::from_reader(zip.by_name(FILEBOX_FILE)?)?;
    if store.list().await?.iter().any(|s| s.id == filebox.id) {
        filebox.id = Uuid::new_v4();
    }
    for attachment in filebox.attachments() {
        let mut bytes = Vec::new();
        zip.by_name(&format!("{ATTACHMENTS_DIR}/{attachment}"))?
            .read_to_end(&mut bytes)?;
        store.put_attachment(filebox.id, attachment, &bytes).await?;
    }
    store.save(&filebox).await?;
    Ok(filebox)
}

#[cfg(test)]
mod tests {
    use pollster::block_on;

    use super::*;
    use crate::model::{Block, Content, Folder, Name, Page, Section};
    use crate::store::fs::FsStore;

    fn name(s: &str) -> Name {
        Name::new(s).unwrap()
    }

    /// A stored filebox with an image and a file attachment, plus one unused attachment.
    fn stored(store: &FsStore) -> (Filebox, Vec<(Id, Vec<u8>)>) {
        let mut fb = Filebox::new(name("Notes"));
        let mut section = Section::new(name("Section"));
        let mut folder = Folder::new(name("Folder"));
        let mut page = Page::new(name("Page"));
        let (image, file) = (Uuid::new_v4(), Uuid::new_v4());
        page.blocks = vec![
            Block::new(Content::Text {
                html: "<p><b>Hi</b></p>".into(),
            }),
            Block::new(Content::Image {
                attachment: image,
                caption: "pic".into(),
            }),
            Block::new(Content::File {
                attachment: file,
                file_name: "data.bin".into(),
            }),
        ];
        folder.pages.push(page);
        section.folders.push(folder);
        fb.sections.push(section);

        let attachments = vec![
            (image, b"\x89PNG fake image".to_vec()),
            (file, (0..=255).cycle().take(5000).collect()),
        ];
        for (id, bytes) in &attachments {
            block_on(store.put_attachment(fb.id, *id, bytes)).unwrap();
        }
        block_on(store.save(&fb)).unwrap();
        (fb, attachments)
    }

    fn store() -> (tempfile::TempDir, FsStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = FsStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn export_then_import_into_empty_store_is_identical() {
        let (_from_dir, from) = store();
        let (fb, attachments) = stored(&from);
        let bytes = block_on(export(&from, fb.id)).unwrap();

        let (_to_dir, to) = store();
        let imported = block_on(import(&to, &bytes)).unwrap();
        assert_eq!(imported, fb);
        assert_eq!(block_on(to.load(fb.id)).unwrap(), fb);
        for (id, expected) in attachments {
            assert_eq!(block_on(to.get_attachment(fb.id, id)).unwrap(), expected);
        }
    }

    #[test]
    fn import_over_existing_id_makes_a_copy() {
        let (_dir, store) = store();
        let (fb, attachments) = stored(&store);
        let bytes = block_on(export(&store, fb.id)).unwrap();

        let copy = block_on(import(&store, &bytes)).unwrap();
        assert_ne!(copy.id, fb.id);
        assert_eq!(
            Filebox {
                id: fb.id,
                ..copy.clone()
            },
            fb
        );
        assert_eq!(block_on(store.load(fb.id)).unwrap(), fb);
        assert_eq!(block_on(store.load(copy.id)).unwrap(), copy);
        for (id, expected) in attachments {
            assert_eq!(
                block_on(store.get_attachment(copy.id, id)).unwrap(),
                expected
            );
        }
        assert_eq!(block_on(store.list()).unwrap().len(), 2);
    }

    #[test]
    fn archive_layout_matches_store_layout() {
        let (_dir, store) = store();
        let (fb, attachments) = stored(&store);
        let bytes = block_on(export(&store, fb.id)).unwrap();
        let zip = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut names: Vec<_> = zip.file_names().map(|n| n.unwrap().to_string()).collect();
        names.sort();
        let mut expected: Vec<_> = attachments
            .iter()
            .map(|(id, _)| format!("attachments/{id}"))
            .chain(["filebox.json".to_string()])
            .collect();
        expected.sort();
        assert_eq!(names, expected);
    }

    #[test]
    fn import_rejects_files_that_are_not_fileboxes() {
        let (_dir, store) = store();
        assert!(block_on(import(&store, b"not a zip")).is_err());

        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("other.txt", SimpleFileOptions::default())
            .unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(block_on(import(&store, &bytes)).is_err());
        assert!(block_on(store.list()).unwrap().is_empty());
    }
}
