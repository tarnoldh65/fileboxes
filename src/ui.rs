mod blocks;
mod files;
mod state;

use dioxus::prelude::*;

use crate::model::Item;
use state::{AppState, Menu, Target, resolve};

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[component]
pub fn App() -> Element {
    let state = use_context_provider(|| AppState::new(state::open_store()));
    state::use_autosave(state);
    use_future(move || state.start());
    #[cfg(feature = "desktop")]
    save_on_close(state);
    rsx! {
        document::Title { "Fileboxes" }
        document::Stylesheet { href: MAIN_CSS }
        Shell {}
    }
}

/// Saves right away when the window closes, so the last second of edits is not lost.
#[cfg(feature = "desktop")]
fn save_on_close(mut state: AppState) {
    use dioxus::desktop::{WindowEvent, tao::event::Event};
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            state.flush_blocking();
        }
    });
}

#[component]
fn Shell() -> Element {
    let mut state = use_context::<AppState>();
    let sel = state.selection();
    let filebox = state.filebox.read();
    let (section, folder, page) = match filebox.as_ref() {
        Some(fb) => resolve(fb, &sel),
        None => (None, None, None),
    };

    rsx! {
        header { class: "topbar",
            FileboxPicker {}
            nav { class: "sections",
                if let Some(fb) = filebox.as_ref() {
                    for s in &fb.sections {
                        Tab {
                            key: "{s.id}",
                            target: Target::Item(Item::Section(s.id)),
                            name: s.name.to_string(),
                            selected: section.is_some_and(|c| c.id == s.id),
                            onselect: {
                                let id = s.id;
                                move |_| state.select(|sel| sel.section = Some(id))
                            },
                        }
                    }
                    button { class: "add", title: "New section", onclick: move |_| state.add_section(), "+" }
                }
            }
            if let Some(error) = state.error.read().as_ref() {
                span { class: "error", title: "Dismiss", onclick: move |_| state.error.set(None), "{error}" }
            }
        }
        main { class: "body",
            nav { class: "folders",
                if let Some(section) = section {
                    for f in &section.folders {
                        Tab {
                            key: "{f.id}",
                            target: Target::Item(Item::Folder(f.id)),
                            name: f.name.to_string(),
                            selected: folder.is_some_and(|c| c.id == f.id),
                            onselect: {
                                let id = f.id;
                                move |_| state.select(|sel| sel.folder = Some(id))
                            },
                        }
                    }
                    button { class: "add", title: "New folder", onclick: move |_| state.add_folder(), "+" }
                }
            }
            div { class: "pages-panel",
                ul { class: "pages",
                    if let Some(folder) = folder {
                        for p in &folder.pages {
                            li { key: "{p.id}", class: "page-item",
                                Tab {
                                    target: Target::Item(Item::Page(p.id)),
                                    name: p.title.to_string(),
                                    selected: page.is_some_and(|c| c.id == p.id),
                                    onselect: {
                                        let id = p.id;
                                        move |_| state.select(|sel| sel.page = Some(id))
                                    },
                                }
                            }
                        }
                    }
                }
                if folder.is_some() {
                    button { class: "add", title: "New page", onclick: move |_| state.add_page(), "+" }
                }
            }
            article { class: "page",
                if let Some(page) = page {
                    blocks::PageView { key: "{page.id}", page: page.clone() }
                } else if filebox.is_none() {
                    p { class: "hint", "Create a filebox with + to get started." }
                }
            }
        }
        ContextMenu {}
        ConfirmDelete {}
    }
}

#[component]
fn FileboxPicker() -> Element {
    let mut state = use_context::<AppState>();
    let current = state.filebox.read().as_ref().map(|fb| fb.id);
    let renaming = *state.renaming.read() == Some(Target::Filebox);

    rsx! {
        if renaming {
            RenameInput {
                target: Target::Filebox,
                name: state.filebox.read().as_ref().map(|fb| fb.name.to_string()).unwrap_or_default(),
            }
        } else {
            select {
                class: "filebox-select",
                onchange: move |e| {
                    if let Ok(id) = e.value().parse() {
                        spawn(state.open(id));
                    }
                },
                for b in state.boxes.read().iter() {
                    option { key: "{b.id}", value: "{b.id}", selected: current == Some(b.id), "{b.name}" }
                }
            }
        }
        button { class: "add", title: "New filebox", onclick: move |_| { spawn(state.new_filebox()); }, "+" }
        label { class: "button", title: "Import a .filebox file",
            "Import"
            input {
                r#type: "file",
                accept: ".filebox",
                hidden: true,
                onchange: move |e| {
                    for file in e.files() {
                        spawn(state.import_filebox(file));
                    }
                },
            }
        }
        if current.is_some() {
            button {
                title: "Filebox actions",
                onclick: move |e| {
                    let p = e.client_coordinates();
                    state.menu.set(Some(Menu { target: Target::Filebox, x: p.x, y: p.y }));
                },
                "..."
            }
        }
        if cfg!(not(target_arch = "wasm32")) {
            button {
                title: "Choose where fileboxes are stored",
                onclick: move |_| {
                    #[cfg(not(target_arch = "wasm32"))]
                    spawn(state.choose_folder());
                },
                "Storage folder"
            }
        }
    }
}

/// A selectable name: click selects, double-click renames, right-click opens the menu.
#[component]
fn Tab(target: Target, name: String, selected: bool, onselect: EventHandler) -> Element {
    let mut state = use_context::<AppState>();
    if *state.renaming.read() == Some(target) {
        return rsx! { RenameInput { target, name } };
    }
    rsx! {
        button {
            class: if selected { "tab selected" } else { "tab" },
            onclick: move |_| onselect.call(()),
            ondoubleclick: move |_| state.renaming.set(Some(target)),
            oncontextmenu: move |e| {
                e.prevent_default();
                let p = e.client_coordinates();
                state.menu.set(Some(Menu { target, x: p.x, y: p.y }));
            },
            "{name}"
        }
    }
}

#[component]
fn RenameInput(target: Target, name: String) -> Element {
    let mut state = use_context::<AppState>();
    let mut value = use_signal(|| name);
    rsx! {
        input {
            class: "rename",
            value: "{value}",
            onmounted: move |e| async move {
                _ = e.set_focus(true).await;
            },
            oninput: move |e| value.set(e.value()),
            onkeydown: move |e| match e.key() {
                Key::Enter => state.rename(target, &value.peek()),
                Key::Escape => state.renaming.set(None),
                _ => {}
            },
            onblur: move |_| state.rename(target, &value.peek()),
        }
    }
}

#[component]
fn ContextMenu() -> Element {
    let mut state = use_context::<AppState>();
    let Some(Menu { target, x, y }) = *state.menu.read() else {
        return rsx! {};
    };
    rsx! {
        div {
            class: "overlay",
            onclick: move |_| state.menu.set(None),
            oncontextmenu: move |e| {
                e.prevent_default();
                state.menu.set(None);
            },
        }
        div { class: "menu", style: "left: {x}px; top: {y}px;",
            button {
                onclick: move |_| {
                    state.menu.set(None);
                    state.renaming.set(Some(target));
                },
                "Rename"
            }
            button {
                onclick: move |_| {
                    state.menu.set(None);
                    state.deleting.set(Some(target));
                },
                "Delete"
            }
            if target == Target::Filebox {
                button {
                    onclick: move |_| {
                        state.menu.set(None);
                        spawn(state.export_filebox());
                    },
                    "Export"
                }
            }
        }
    }
}

#[component]
fn ConfirmDelete() -> Element {
    let mut state = use_context::<AppState>();
    let Some(target) = *state.deleting.read() else {
        return rsx! {};
    };
    let name = state
        .filebox
        .read()
        .as_ref()
        .and_then(|fb| match target {
            Target::Filebox => Some(fb.name.to_string()),
            Target::Item(item) => fb.name_of(item).map(|n| n.to_string()),
        })
        .unwrap_or_default();
    rsx! {
        div { class: "overlay dim", onclick: move |_| state.deleting.set(None) }
        div { class: "dialog",
            p { "Delete \"{name}\" and everything in it?" }
            button { class: "danger", onclick: move |_| state.delete(target), "Delete" }
            button { onclick: move |_| state.deleting.set(None), "Cancel" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Block, Content, Filebox, Folder, Name, Page, Section, Table};
    use crate::store::Summary;
    use crate::store::fs::FsStore;
    use state::Selection;

    fn name(s: &str) -> Name {
        Name::new(s).unwrap()
    }

    fn sample() -> Filebox {
        let mut fb = Filebox::new(name("Work"));
        for (s, folders) in [("Projects", vec!["Alpha", "Beta"]), ("Admin", vec!["HR"])] {
            let mut section = Section::new(name(s));
            for f in folders {
                let mut folder = Folder::new(name(f));
                for p in ["One", "Two", "Three"] {
                    folder.pages.push(Page::new(name(&format!("{f} {p}"))));
                }
                section.folders.push(folder);
            }
            fb.sections.push(section);
        }
        fb
    }

    #[derive(Props, Clone, PartialEq)]
    struct RootProps {
        filebox: Filebox,
        selection: Selection,
    }

    #[allow(non_snake_case)]
    fn Root(props: RootProps) -> Element {
        use_context_provider(|| {
            let state = AppState::new(FsStore::open(std::env::temp_dir()).unwrap());
            let mut boxes = state.boxes;
            boxes.set(vec![
                Summary {
                    id: props.filebox.id,
                    name: props.filebox.name.clone(),
                },
                Summary {
                    id: uuid::Uuid::new_v4(),
                    name: name("Zeta"),
                },
            ]);
            let mut selections = state.selections;
            selections.write().insert(props.filebox.id, props.selection);
            let mut filebox = state.filebox;
            filebox.set(Some(props.filebox.clone()));
            state
        });
        rsx! { Shell {} }
    }

    fn render(filebox: Filebox, selection: Selection) -> String {
        let mut dom = VirtualDom::new_with_props(Root, RootProps { filebox, selection });
        dom.rebuild_in_place();
        dioxus_ssr::render(&dom)
    }

    /// Asserts each needle appears exactly once, in the given order.
    fn assert_in_order(html: &str, needles: &[&str]) {
        let mut last = 0;
        for needle in needles {
            assert_eq!(html.matches(needle).count(), 1, "{needle} in {html}");
            let at = html.find(needle).unwrap();
            assert!(at > last, "{needle} out of order");
            last = at;
        }
    }

    #[test]
    fn shell_renders_layout_regions() {
        let html = render(sample(), Selection::default());
        for class in ["filebox-select", "sections", "folders", "pages", "page"] {
            assert!(
                html.contains(&format!("class=\"{class}\"")),
                "missing {class}"
            );
        }
    }

    #[test]
    fn shell_lists_fileboxes_sections_folders_and_pages_in_order() {
        let html = render(sample(), Selection::default());
        assert_in_order(
            &html,
            &[
                ">Work</option>",
                ">Zeta</option>",
                ">Projects</button>",
                ">Admin</button>",
                ">Alpha</button>",
                ">Beta</button>",
                ">Alpha One</button>",
                ">Alpha Two</button>",
                ">Alpha Three</button>",
            ],
        );
        assert_eq!(html.matches("class=\"page-item\"").count(), 3);
        assert_eq!(html.matches("class=\"tab selected\"").count(), 3);
        assert!(html.contains("<h1>Alpha One</h1>"));
    }

    #[test]
    fn shell_shows_only_the_selected_section_and_folder() {
        let fb = sample();
        let selection = Selection {
            section: Some(fb.sections[0].id),
            folder: Some(fb.sections[0].folders[1].id),
            page: Some(fb.sections[0].folders[1].pages[2].id),
        };
        let html = render(fb, selection);
        assert!(!html.contains(">HR</button>"));
        assert!(!html.contains(">Alpha One</button>"));
        assert_in_order(&html, &[">Beta One</button>", ">Beta Three</button>"]);
        assert!(html.contains("<h1>Beta Three</h1>"));
    }

    #[test]
    fn page_renders_every_block_type() {
        let mut fb = sample();
        let mut table = Table::default();
        table.add_column();
        table.rows[1][2] = "cell text".into();
        let mut code = Block::new(Content::Code {
            text: "let x = 1;".into(),
        });
        code.half_width = true;
        fb.sections[0].folders[0].pages[0].blocks = vec![
            Block::new(Content::Text {
                html: "<p><b>Bold</b> words</p>".into(),
            }),
            Block::new(Content::Table(table)),
            Block::new(Content::Image {
                attachment: uuid::Uuid::new_v4(),
                caption: "A caption".into(),
            }),
            Block::new(Content::File {
                attachment: uuid::Uuid::new_v4(),
                file_name: "report.pdf".into(),
            }),
            code,
        ];
        let html = render(fb, Selection::default());
        assert_in_order(
            &html,
            &[
                "<p><b>Bold</b> words</p>",
                "value=\"cell text\"",
                "value=\"A caption\"",
                ">report.pdf</button>",
                "value=\"let x = 1;\"></textarea>",
            ],
        );
        assert!(html.contains("contenteditable=\"true\""));
        assert_eq!(html.matches("<td>").count(), 6);
        assert_eq!(html.matches("class=\"block half\"").count(), 1);
        assert_eq!(html.matches("class=\"block\"").count(), 4);
    }
}
