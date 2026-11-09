//! Page editor: the formatting toolbar and one component per block type.

use base64::{Engine, engine::general_purpose::STANDARD};
use dioxus::prelude::*;

use super::files::open_file;
use super::state::AppState;
use crate::model::{Block, Content, Id, Page, Table};
use crate::sanitize::sanitize;

#[component]
pub fn PageView(page: Page) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        h1 { "{page.title}" }
        Toolbar {}
        div { class: "blocks",
            for block in page.blocks {
                BlockView { key: "{block.id}", block }
            }
        }
        div { class: "add-blocks",
            button { onclick: move |_| state.add_block(Content::Text { html: String::new() }), "+ Text" }
            button { onclick: move |_| state.add_block(Content::Table(Table::default())), "+ Table" }
            label { class: "button",
                "+ Image"
                input {
                    r#type: "file",
                    accept: "image/*",
                    hidden: true,
                    onchange: move |e| {
                        for file in e.files() {
                            spawn(state.add_attachment(file, true));
                        }
                    },
                }
            }
            label { class: "button",
                "+ File"
                input {
                    r#type: "file",
                    hidden: true,
                    onchange: move |e| {
                        for file in e.files() {
                            spawn(state.add_attachment(file, false));
                        }
                    },
                }
            }
            button { onclick: move |_| state.add_block(Content::Code { text: String::new() }), "+ Code" }
        }
    }
}

/// Builds a script that runs a browser editing command on the current selection.
fn exec(command: &str, arg: &str, css: bool) -> String {
    format!(
        "document.execCommand('styleWithCSS', false, {css}); \
         document.execCommand('{command}', false, '{arg}');"
    )
}

const COLORS: [(&str, &str); 6] = [
    ("Black", "#222222"),
    ("Red", "#c00000"),
    ("Orange", "#d06000"),
    ("Green", "#008000"),
    ("Blue", "#0050c0"),
    ("Purple", "#8000a0"),
];

/// Formatting buttons for whichever text block has the cursor.
#[component]
fn Toolbar() -> Element {
    let link = "const url = prompt('Link URL'); \
                if (url) { document.execCommand('styleWithCSS', false, false); \
                document.execCommand('createLink', false, url); }";
    rsx! {
        div { class: "toolbar",
            Cmd { title: "Bold", script: exec("bold", "", false), b { "B" } }
            Cmd { title: "Italic", script: exec("italic", "", false), i { "I" } }
            Cmd { title: "Underline", script: exec("underline", "", false), u { "U" } }
            span { class: "sep" }
            Cmd { title: "Small text", script: exec("fontSize", "2", true), "Small" }
            Cmd { title: "Normal text", script: exec("fontSize", "3", true), "Normal" }
            Cmd { title: "Large text", script: exec("fontSize", "5", true), "Large" }
            Cmd { title: "Huge text", script: exec("fontSize", "6", true), "Huge" }
            span { class: "sep" }
            for (name, color) in COLORS {
                Cmd { title: name, script: exec("foreColor", color, true),
                    span { class: "swatch", style: "background: {color};" }
                }
            }
            span { class: "sep" }
            Cmd { title: "Align left", script: exec("justifyLeft", "", true), "Left" }
            Cmd { title: "Align center", script: exec("justifyCenter", "", true), "Center" }
            Cmd { title: "Align right", script: exec("justifyRight", "", true), "Right" }
            span { class: "sep" }
            Cmd { title: "Bulleted list", script: exec("insertUnorderedList", "", false), "Bullets" }
            Cmd { title: "Numbered list", script: exec("insertOrderedList", "", false), "Numbers" }
            Cmd { title: "Link", script: link, "Link" }
        }
    }
}

/// A toolbar button. It acts on mouse down and cancels the default so the text block
/// keeps its focus and selection.
#[component]
fn Cmd(title: &'static str, script: String, children: Element) -> Element {
    rsx! {
        button {
            title,
            onmousedown: move |e| {
                e.prevent_default();
                document::eval(&script);
            },
            {children}
        }
    }
}

#[component]
fn BlockView(block: Block) -> Element {
    let mut state = use_context::<AppState>();
    let id = block.id;
    let half = block.half_width;
    rsx! {
        div { class: if half { "block half" } else { "block" },
            div { class: "block-tools",
                button { title: "Move up", onclick: move |_| state.move_block(id, -1), "↑" }
                button { title: "Move down", onclick: move |_| state.move_block(id, 1), "↓" }
                button {
                    onclick: move |_| state.edit_block(id, |b| b.half_width = !b.half_width),
                    if half { "Full width" } else { "Half width" }
                }
                button { title: "Delete block", onclick: move |_| state.delete_block(id), "Delete" }
            }
            match block.content {
                Content::Text { html } => rsx! { TextBlock { id, html } },
                Content::Table(table) => rsx! { TableBlock { id, table } },
                Content::Image { attachment, caption } => rsx! { ImageBlock { id, attachment, caption } },
                Content::File { attachment, file_name } => rsx! { FileBlock { attachment, file_name } },
                Content::Code { text } => rsx! { CodeBlock { id, text } },
            }
        }
    }
}

#[component]
fn TextBlock(id: Id, html: String) -> Element {
    let mut state = use_context::<AppState>();
    // Set the HTML only once: replacing it while typing would move the cursor.
    let initial = use_hook(|| html);
    rsx! {
        div {
            id: "block-{id}",
            class: "text",
            contenteditable: "true",
            dangerous_inner_html: "{initial}",
            oninput: move |_| async move {
                let script = format!("return document.getElementById('block-{id}').innerHTML;");
                if let Ok(html) = document::eval(&script).join::<String>().await {
                    let html = sanitize(&html);
                    state.edit_block(id, |b| b.content = Content::Text { html });
                }
            },
        }
    }
}

fn edit_table(mut state: AppState, id: Id, change: impl FnOnce(&mut Table)) {
    state.edit_block(id, |b| {
        if let Content::Table(table) = &mut b.content {
            change(table);
        }
    });
}

#[component]
fn TableBlock(id: Id, table: Table) -> Element {
    let state = use_context::<AppState>();
    rsx! {
        table { class: "grid",
            for (r, row) in table.rows.iter().enumerate() {
                tr {
                    for (c, cell) in row.iter().enumerate() {
                        td {
                            input {
                                value: "{cell}",
                                oninput: move |e| {
                                    let value = e.value();
                                    edit_table(state, id, |t| t.rows[r][c] = value);
                                },
                            }
                        }
                    }
                }
            }
        }
        div { class: "table-tools",
            button { onclick: move |_| edit_table(state, id, Table::add_row), "+ Row" }
            button { onclick: move |_| edit_table(state, id, Table::remove_row), "- Row" }
            button { onclick: move |_| edit_table(state, id, Table::add_column), "+ Column" }
            button { onclick: move |_| edit_table(state, id, Table::remove_column), "- Column" }
        }
    }
}

/// The MIME type of an image, from its first bytes. SVG has no magic number, so it is
/// the fallback.
fn image_mime(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, 0xFF, ..] => "image/jpeg",
        [b'G', b'I', b'F', b'8', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        [b'B', b'M', ..] => "image/bmp",
        _ => "image/svg+xml",
    }
}

fn image_url(bytes: &[u8]) -> String {
    format!(
        "data:{};base64,{}",
        image_mime(bytes),
        STANDARD.encode(bytes)
    )
}

#[component]
fn ImageBlock(id: Id, attachment: Id, caption: String) -> Element {
    let mut state = use_context::<AppState>();
    let src = use_resource(move || {
        state.attachments_written.read();
        async move {
            state
                .get_attachment(attachment)
                .await
                .map(|b| image_url(&b))
        }
    });
    rsx! {
        figure { class: "image",
            if let Some(Some(url)) = &*src.read() {
                img { src: "{url}" }
            } else {
                div { class: "hint", "Loading image..." }
            }
            input {
                class: "caption",
                placeholder: "Caption",
                value: "{caption}",
                oninput: move |e| {
                    let value = e.value();
                    state.edit_block(id, |b| {
                        if let Content::Image { caption, .. } = &mut b.content {
                            *caption = value;
                        }
                    });
                },
            }
        }
    }
}

#[component]
fn FileBlock(attachment: Id, file_name: String) -> Element {
    let mut state = use_context::<AppState>();
    let name = file_name.clone();
    rsx! {
        button {
            class: "file",
            title: if cfg!(target_arch = "wasm32") { "Download" } else { "Open" },
            onclick: move |_| {
                let name = name.clone();
                spawn(async move {
                    if let Some(bytes) = state.get_attachment(attachment).await {
                        state.report(open_file(attachment, &name, &bytes));
                    }
                });
            },
            "{file_name}"
        }
    }
}

#[component]
fn CodeBlock(id: Id, text: String) -> Element {
    let mut state = use_context::<AppState>();
    let rows = text.lines().count().max(3) + 1;
    rsx! {
        textarea {
            class: "code",
            spellcheck: "false",
            rows: "{rows}",
            value: "{text}",
            oninput: move |e| {
                let text = e.value();
                state.edit_block(id, |b| b.content = Content::Code { text });
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_mime_detects_common_formats() {
        assert_eq!(image_mime(b"\x89PNG\r\n\x1a\n"), "image/png");
        assert_eq!(image_mime(b"\xFF\xD8\xFF\xE0"), "image/jpeg");
        assert_eq!(image_mime(b"GIF89a"), "image/gif");
        assert_eq!(image_mime(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
        assert_eq!(image_mime(b"BM\0\0"), "image/bmp");
        assert_eq!(image_mime(b"<svg xmlns="), "image/svg+xml");
    }

    #[test]
    fn image_url_is_base64_data_url() {
        assert_eq!(image_url(b"GIF8"), "data:image/gif;base64,R0lGOA==");
    }

    #[test]
    fn exec_builds_editing_script() {
        assert_eq!(
            exec("foreColor", "#c00000", true),
            "document.execCommand('styleWithCSS', false, true); \
             document.execCommand('foreColor', false, '#c00000');"
        );
    }
}
