//! Storage tests shared by every `Store` backend. Each check creates its own fileboxes,
//! so they can all run against one store.

use std::collections::HashSet;

use fileboxes::model::{Block, Content, Filebox, Folder, Id, Name, Page, Section};
use fileboxes::store::Store;
use uuid::Uuid;

pub async fn run(store: &impl Store) {
    save_then_load_round_trips(store).await;
    save_overwrites_previous_version(store).await;
    attachments_round_trip_byte_for_byte(store).await;
    save_removes_unreferenced_attachments(store).await;
    list_returns_every_filebox_sorted_by_name(store).await;
    delete_removes_filebox(store).await;
    missing_items_are_errors(store).await;
}

fn name(s: &str) -> Name {
    Name::new(s).unwrap()
}

fn filebox(title: &str) -> Filebox {
    let mut fb = Filebox::new(name(title));
    let mut section = Section::new(name("Section"));
    let mut folder = Folder::new(name("Folder"));
    let mut page = Page::new(name("Page"));
    page.blocks.push(Block::new(Content::Text {
        html: "<p>Hello</p>".into(),
    }));
    folder.pages.push(page);
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

async fn save_then_load_round_trips(store: &impl Store) {
    let fb = filebox("Round trip");
    store.save(&fb).await.unwrap();
    assert_eq!(store.load(fb.id).await.unwrap(), fb);
}

async fn save_overwrites_previous_version(store: &impl Store) {
    let mut fb = filebox("Before");
    store.save(&fb).await.unwrap();
    fb.name = name("After");
    blocks(&mut fb).clear();
    store.save(&fb).await.unwrap();
    assert_eq!(store.load(fb.id).await.unwrap(), fb);
}

async fn attachments_round_trip_byte_for_byte(store: &impl Store) {
    let mut fb = filebox("Attachments");
    let id = Uuid::new_v4();
    let bytes: Vec<u8> = (0..=255).cycle().take(10_000).collect();
    store.put_attachment(fb.id, id, &bytes).await.unwrap();
    blocks(&mut fb).push(image(id));
    store.save(&fb).await.unwrap();
    assert_eq!(store.get_attachment(fb.id, id).await.unwrap(), bytes);
}

async fn save_removes_unreferenced_attachments(store: &impl Store) {
    let mut fb = filebox("Cleanup");
    let (kept, dropped) = (Uuid::new_v4(), Uuid::new_v4());
    store.put_attachment(fb.id, kept, b"keep").await.unwrap();
    store.put_attachment(fb.id, dropped, b"drop").await.unwrap();
    blocks(&mut fb).extend([image(kept), image(dropped)]);
    store.save(&fb).await.unwrap();
    assert_eq!(store.get_attachment(fb.id, dropped).await.unwrap(), b"drop");

    // Deleting the block frees its attachment on the next save.
    blocks(&mut fb).pop();
    store.save(&fb).await.unwrap();
    assert_eq!(store.get_attachment(fb.id, kept).await.unwrap(), b"keep");
    assert!(store.get_attachment(fb.id, dropped).await.is_err());
}

async fn list_returns_every_filebox_sorted_by_name(store: &impl Store) {
    let (b, a) = (filebox("List Beta"), filebox("List Alpha"));
    store.save(&b).await.unwrap();
    store.save(&a).await.unwrap();
    let list = store.list().await.unwrap();

    let ids: HashSet<_> = list.iter().map(|s| s.id).collect();
    assert!(ids.contains(&a.id) && ids.contains(&b.id));
    let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted);
    let alpha = list.iter().find(|s| s.id == a.id).unwrap();
    assert_eq!(alpha.name, a.name);
}

async fn delete_removes_filebox(store: &impl Store) {
    let fb = filebox("Deleted");
    let attachment = Uuid::new_v4();
    store.put_attachment(fb.id, attachment, b"x").await.unwrap();
    store.save(&fb).await.unwrap();
    store.delete(fb.id).await.unwrap();
    assert!(store.load(fb.id).await.is_err());
    assert!(store.get_attachment(fb.id, attachment).await.is_err());
    assert!(store.list().await.unwrap().iter().all(|s| s.id != fb.id));
}

async fn missing_items_are_errors(store: &impl Store) {
    let fb = filebox("Missing");
    store.save(&fb).await.unwrap();
    assert!(store.load(Uuid::new_v4()).await.is_err());
    assert!(store.get_attachment(fb.id, Uuid::new_v4()).await.is_err());
    assert!(
        store
            .get_attachment(Uuid::new_v4(), Uuid::new_v4())
            .await
            .is_err()
    );
}
