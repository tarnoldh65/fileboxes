//! Import of OneNote notebooks (`.onepkg`) and sections (`.one`).
//!
//! Reading a file produces plain values ([`Entry`] and the types below it); converting
//! those values into a filebox does not depend on the OneNote parser, so it is tested
//! with hand-built values.

use std::io::{self, Read};

use onenote_parser::contents::{
    Content as OneContent, EmbeddedFile, Image, OutlineElement, OutlineItem, RichText,
};
use onenote_parser::page::{Page as OnePage, PageContent};
use onenote_parser::property::common::ColorRef;
use onenote_parser::property::rich_text::ParagraphAlignment;
use onenote_parser::section::{Section as OneSection, SectionEntry};
use onenote_parser::{FileSystem, Parser};
use typed_path::{TypedPath, TypedPathBuf};
use uuid::Uuid;

use crate::model::{Block, Content, Filebox, Folder, Id, Name, Page, Section, Table};
use crate::sanitize::{escape, sanitize};
use crate::store::Store;

/// File extensions this module imports.
pub const EXTENSIONS: [&str; 2] = ["onepkg", "one"];

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Group { name: String, entries: Vec<Entry> },
    Section { name: String, pages: Vec<NotePage> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct NotePage {
    pub title: String,
    pub containers: Vec<Container>,
}

/// Something placed on a page: a text container, or an image or file on its own.
#[derive(Debug, Clone, PartialEq)]
pub struct Container {
    pub x: f32,
    pub y: f32,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Paragraph(Paragraph),
    /// Rows of cells, each cell a list of paragraphs.
    Table(Vec<Vec<Vec<Paragraph>>>),
    Image {
        bytes: Vec<u8>,
        caption: String,
    },
    File {
        bytes: Vec<u8>,
        name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum List {
    Bullets,
    Numbers,
}

impl List {
    fn tag(self) -> &'static str {
        match self {
            List::Bullets => "ul",
            List::Numbers => "ol",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Paragraph {
    pub runs: Vec<Run>,
    /// 1 to 6 for Heading 1 to Heading 6.
    pub heading: Option<u8>,
    /// `center` or `right`; `None` is left.
    pub align: Option<&'static str>,
    pub list: Option<List>,
    /// Indent level; nested list items have a higher level than their parent.
    pub level: u8,
}

impl Paragraph {
    fn is_empty(&self) -> bool {
        self.runs.iter().all(|r| r.text.trim().is_empty())
    }
}

/// A stretch of text with one formatting.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub color: Option<[u8; 3]>,
    /// Font size in points.
    pub size: Option<f32>,
    pub link: Option<String>,
}

/// OneNote's default font size, which is left out of the HTML.
const DEFAULT_SIZE: f32 = 11.0;

// ---- Conversion ----

/// A converted filebox and the bytes of its attachments.
pub struct Converted {
    pub filebox: Filebox,
    pub attachments: Vec<(Id, Vec<u8>)>,
}

fn name(text: &str, fallback: &str) -> Name {
    Name::new(text)
        .or_else(|_| Name::new(fallback))
        .expect("fallback names are not empty")
}

/// Builds a filebox named `notebook`. Sections outside any group become folders of one
/// section named after the notebook; each group becomes a section of its own.
pub fn convert(notebook: &str, entries: Vec<Entry>) -> Converted {
    let mut converted = Converted {
        filebox: Filebox::new(name(notebook, "OneNote")),
        attachments: Vec::new(),
    };
    let mut loose = Section::new(name(notebook, "OneNote"));
    for entry in entries {
        match entry {
            Entry::Section { name, pages } => loose.folders.push(converted.folder(&name, pages)),
            Entry::Group { name, entries } => converted.add_group(name, entries),
        }
    }
    if !loose.folders.is_empty() {
        converted.filebox.sections.insert(0, loose);
    }
    converted
}

impl Converted {
    /// Adds a section for the group at `path`, then one for each group nested in it.
    fn add_group(&mut self, path: String, entries: Vec<Entry>) {
        let mut section = Section::new(name(&path, "Section group"));
        let mut groups = Vec::new();
        for entry in entries {
            match entry {
                Entry::Section { name, pages } => section.folders.push(self.folder(&name, pages)),
                Entry::Group { name, entries } => {
                    groups.push((format!("{path} / {name}"), entries))
                }
            }
        }
        self.filebox.sections.push(section);
        for (path, entries) in groups {
            self.add_group(path, entries);
        }
    }

    fn folder(&mut self, folder_name: &str, pages: Vec<NotePage>) -> Folder {
        let mut folder = Folder::new(name(folder_name, "Section"));
        for note in pages {
            let mut page = Page::new(name(&note.title, "Untitled page"));
            page.blocks = self.blocks(note.containers);
            folder.pages.push(page);
        }
        folder
    }

    fn attach(&mut self, bytes: Vec<u8>) -> Id {
        let id = Uuid::new_v4();
        self.attachments.push((id, bytes));
        id
    }

    /// Containers in reading order, top to bottom then left to right. Paragraphs in a
    /// row become one text block; tables, images, and files are blocks of their own.
    fn blocks(&mut self, mut containers: Vec<Container>) -> Vec<Block> {
        containers.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        let mut blocks = Vec::new();
        for container in containers {
            let mut text = Vec::new();
            for item in container.items {
                let content = match item {
                    Item::Paragraph(p) => {
                        text.push(p);
                        continue;
                    }
                    Item::Table(rows) => Content::Table(table(&rows)),
                    Item::Image { bytes, caption } => Content::Image {
                        attachment: self.attach(bytes),
                        caption,
                    },
                    Item::File { bytes, name } => Content::File {
                        attachment: self.attach(bytes),
                        file_name: name,
                    },
                };
                push_text(&mut blocks, &mut text);
                blocks.push(Block::new(content));
            }
            push_text(&mut blocks, &mut text);
        }
        blocks
    }
}

fn push_text(blocks: &mut Vec<Block>, text: &mut Vec<Paragraph>) {
    let html = paragraphs_html(text);
    text.clear();
    if !html.is_empty() {
        blocks.push(Block::new(Content::Text {
            html: sanitize(&html),
        }));
    }
}

/// A table with every row padded to the widest one. A cell's paragraphs are joined
/// with line breaks.
fn table(rows: &[Vec<Vec<Paragraph>>]) -> Table {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let mut cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let mut cells: Vec<String> = row
                .iter()
                .map(|cell| {
                    let lines: Vec<String> = cell
                        .iter()
                        .filter(|p| !p.is_empty())
                        .map(|p| inline_html(&p.runs))
                        .collect();
                    sanitize(&lines.join("<br>"))
                })
                .collect();
            cells.resize(columns, String::new());
            cells
        })
        .collect();
    if cells.is_empty() {
        cells.push(vec![String::new()]);
    }
    Table { cells }
}

/// Paragraphs as HTML: list items are nested by level; other paragraphs become
/// headings, aligned divs, or plain paragraphs. Empty paragraphs are dropped.
fn paragraphs_html(paragraphs: &[Paragraph]) -> String {
    let mut html = String::new();
    // The lists open around the current item, innermost last.
    let mut open: Vec<(List, u8)> = Vec::new();
    for p in paragraphs.iter().filter(|p| !p.is_empty()) {
        let content = inline_html(&p.runs);
        let Some(list) = p.list else {
            while let Some((list, _)) = open.pop() {
                html += &format!("</li></{}>", list.tag());
            }
            let tag = match (p.heading, p.align) {
                (Some(n), _) => format!("h{n}"),
                (None, Some(_)) => "div".into(),
                (None, None) => "p".into(),
            };
            let style = p
                .align
                .map(|a| format!(" style=\"text-align:{a}\""))
                .unwrap_or_default();
            html += &format!("<{tag}{style}>{content}</{tag}>");
            continue;
        };
        while let Some(&(other, level)) = open.last()
            && (level > p.level || (level == p.level && other != list))
        {
            html += &format!("</li></{}>", other.tag());
            open.pop();
        }
        if open.last().is_some_and(|&(_, level)| level == p.level) {
            html += "</li><li>";
        } else {
            html += &format!("<{}><li>", list.tag());
            open.push((list, p.level));
        }
        html += &content;
    }
    while let Some((list, _)) = open.pop() {
        html += &format!("</li></{}>", list.tag());
    }
    html
}

/// Runs as inline HTML. Leading spaces of the first run are kept, so indented lines
/// such as pasted commands keep their indent; trailing spaces of the last are dropped.
fn inline_html(runs: &[Run]) -> String {
    let last = runs.len().saturating_sub(1);
    runs.iter()
        .enumerate()
        .map(|(i, run)| run_html(run, i == 0, i == last))
        .collect()
}

fn run_html(run: &Run, first: bool, last: bool) -> String {
    let mut text = if last { run.text.trim_end() } else { &run.text }.to_string();
    if first {
        let rest = text.trim_start_matches(' ');
        text = "\u{a0}".repeat(text.len() - rest.len()) + rest;
    }
    let mut html = escape(&text).replace(['\u{b}', '\n', '\r'], "<br>");
    // OneNote links typed URLs without storing a separate target.
    let trimmed = text.trim();
    let bare_url =
        (trimmed.starts_with("http://") || trimmed.starts_with("https://")).then_some(trimmed);
    if let Some(link) = run.link.as_deref().or(bare_url) {
        html = format!("<a href=\"{}\">{html}</a>", escape(link));
    }
    let style: Vec<String> = [
        run.color
            .map(|[r, g, b]| format!("color:#{r:02x}{g:02x}{b:02x}")),
        run.size
            .filter(|&s| s != DEFAULT_SIZE)
            .map(|s| format!("font-size:{s}pt")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !style.is_empty() {
        html = format!("<span style=\"{}\">{html}</span>", style.join(";"));
    }
    for (on, tag) in [
        (run.bold, "b"),
        (run.italic, "i"),
        (run.underline, "u"),
        (run.strike, "s"),
    ] {
        if on {
            html = format!("<{tag}>{html}</{tag}>");
        }
    }
    html
}

/// Stores a converted filebox and its attachments, as a new filebox.
pub async fn save(store: &impl Store, converted: Converted) -> io::Result<Filebox> {
    let Converted {
        filebox,
        attachments,
    } = converted;
    for (id, bytes) in attachments {
        store.put_attachment(filebox.id, id, &bytes).await?;
    }
    store.save(&filebox).await?;
    Ok(filebox)
}

/// Imports a `.onepkg` or `.one` file into the store as a new filebox.
pub async fn import(store: &impl Store, bytes: &[u8], file_name: &str) -> io::Result<Filebox> {
    let entries = read(bytes, file_name)?;
    let notebook = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem);
    save(store, convert(notebook, entries)).await
}

// ---- Reading OneNote files ----

/// Serves the one file being imported to the parser. A package is unpacked in memory,
/// so nothing else is ever read.
#[derive(Clone, Copy)]
struct OneFile<'a>(&'a [u8]);

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "only the imported file is readable",
    )
}

impl FileSystem for OneFile<'_> {
    fn is_directory(&self, _: TypedPath) -> io::Result<bool> {
        Ok(false)
    }

    fn read_dir(&self, _: TypedPath) -> io::Result<Vec<TypedPathBuf>> {
        Ok(Vec::new())
    }

    fn read_file(&self, _: TypedPath) -> io::Result<Vec<u8>> {
        Ok(self.0.to_vec())
    }

    fn write_file(&self, _: TypedPath, _: &[u8]) -> io::Result<()> {
        Err(unsupported())
    }

    fn stream_to_file(&self, _: TypedPath, _: &mut dyn Read) -> io::Result<()> {
        Err(unsupported())
    }

    fn make_dir(&self, _: TypedPath) -> io::Result<()> {
        Err(unsupported())
    }

    fn canonicalize(&self, path: TypedPath) -> io::Result<TypedPathBuf> {
        Ok(path.to_path_buf())
    }

    fn exists(&self, _: TypedPath) -> io::Result<bool> {
        Ok(false)
    }
}

/// Reads the sections and pages of a `.onepkg` notebook or a `.one` section.
pub fn read(bytes: &[u8], file_name: &str) -> io::Result<Vec<Entry>> {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase());
    let path = TypedPath::derive(file_name);
    let parser = Parser::new_with_fs(OneFile(bytes));
    let parsed = match extension.as_deref() {
        Some("onepkg") => parser.parse_package(path).map(|n| n.entries().to_vec()),
        Some("one") => parser
            .parse_section_buffer(bytes, path)
            .map(|s| vec![SectionEntry::Section(s)]),
        _ => return Err(io::Error::other("not a OneNote file")),
    };
    let entries =
        parsed.map_err(|e| io::Error::other(format!("not a readable OneNote file: {e}")))?;
    entries.iter().map(entry).collect()
}

fn entry(entry: &SectionEntry) -> io::Result<Entry> {
    Ok(match entry {
        SectionEntry::Section(s) => section(s)?,
        SectionEntry::SectionGroup(g) => Entry::Group {
            name: g.display_name().into(),
            entries: g
                .entries()
                .iter()
                .map(self::entry)
                .collect::<io::Result<_>>()?,
        },
    })
}

fn section(section: &OneSection) -> io::Result<Entry> {
    Ok(Entry::Section {
        name: section.display_name().into(),
        pages: section
            .page_series()
            .iter()
            .flat_map(|s| s.pages())
            .map(page)
            .collect::<io::Result<_>>()?,
    })
}

fn page(page: &OnePage) -> io::Result<NotePage> {
    let mut containers = Vec::new();
    for content in page.contents() {
        let mut items = Vec::new();
        let (x, y) = match content {
            PageContent::Outline(o) => {
                outline(o.items(), 0, &mut items)?;
                (o.offset_horizontal(), o.offset_vertical())
            }
            PageContent::Image(i) => {
                items.extend(image(i)?);
                (i.offset_horizontal(), i.offset_vertical())
            }
            PageContent::EmbeddedFile(f) => {
                items.push(file(f)?);
                (f.offset_horizontal(), f.offset_vertical())
            }
            _ => continue,
        };
        containers.push(Container {
            x: x.unwrap_or(0.0),
            y: y.unwrap_or(0.0),
            items,
        });
    }
    Ok(NotePage {
        title: page.title_text().unwrap_or_default().into(),
        containers,
    })
}

fn outline(items: &[OutlineItem], level: u8, out: &mut Vec<Item>) -> io::Result<()> {
    for item in items {
        match item {
            OutlineItem::Element(e) => {
                element(e, level, out)?;
                outline(e.children(), level + e.child_level(), out)?;
            }
            OutlineItem::Group(g) => outline(g.outlines(), level + g.child_level(), out)?,
        }
    }
    Ok(())
}

/// OneNote marks numbered lists with this character in the list format.
const NUMBERED: char = '\u{fffd}';

fn element(element: &OutlineElement, level: u8, out: &mut Vec<Item>) -> io::Result<()> {
    let list = element.list_contents().first().map(|l| {
        if l.list_format().first() == Some(&NUMBERED) {
            List::Numbers
        } else {
            List::Bullets
        }
    });
    for content in element.contents() {
        match content {
            OneContent::RichText(t) => out.push(Item::Paragraph(Paragraph {
                list,
                level,
                ..paragraph(t)
            })),
            OneContent::Table(t) => out.push(Item::Table(
                t.contents()
                    .iter()
                    .map(|row| {
                        row.contents()
                            .iter()
                            .map(|cell| {
                                cell.contents()
                                    .iter()
                                    .flat_map(|e| e.contents())
                                    .filter_map(|c| c.rich_text())
                                    .map(paragraph)
                                    .collect()
                            })
                            .collect()
                    })
                    .collect(),
            )),
            OneContent::Image(i) => out.extend(image(i)?),
            OneContent::EmbeddedFile(f) => out.push(file(f)?),
            _ => {}
        }
    }
    Ok(())
}

fn color(color: Option<ColorRef>) -> Option<[u8; 3]> {
    match color? {
        ColorRef::Manual { r, g, b } => Some([r, g, b]),
        ColorRef::Auto => None,
    }
}

fn paragraph(text: &RichText) -> Paragraph {
    let units: Vec<u16> = text.text().encode_utf16().collect();
    let mut bounds: Vec<usize> = text
        .text_run_indices()
        .iter()
        .map(|&i| (i as usize).min(units.len()))
        .collect();
    bounds.push(units.len());
    let links = text.hyperlinks();
    let mut runs = Vec::new();
    let mut start = 0;
    for (i, end) in bounds.into_iter().enumerate() {
        let (from, to) = (start, end.max(start));
        start = to;
        let style = text.text_run_formatting().get(i);
        // Hidden runs hold link targets, not visible text.
        if style.is_some_and(|s| s.hidden()) {
            continue;
        }
        let link = links
            .iter()
            .find(|l| from >= l.start() as usize && to <= l.end() as usize)
            .map(|l| l.target().to_string());
        let text = String::from_utf16_lossy(&units[from..to]);
        runs.push(match style {
            Some(s) => Run {
                text,
                bold: s.bold(),
                italic: s.italic(),
                underline: s.underline(),
                strike: s.strikethrough(),
                color: color(s.font_color()),
                size: s
                    .font_size()
                    .map(|half_points| f32::from(half_points) / 2.0),
                link,
            },
            None => Run {
                text,
                link,
                ..Run::default()
            },
        });
    }
    let heading = text
        .paragraph_style()
        .style_id()
        .and_then(|id| id.strip_prefix('h')?.parse().ok())
        .filter(|n| (1..=6).contains(n));
    Paragraph {
        runs,
        heading,
        align: match text.paragraph_alignment() {
            ParagraphAlignment::Center => Some("center"),
            ParagraphAlignment::Right => Some("right"),
            _ => None,
        },
        list: None,
        level: 0,
    }
}

fn image(image: &Image) -> io::Result<Option<Item>> {
    let Some(mut reader) = image.read() else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(Some(Item::Image {
        bytes,
        caption: image.alt_text().unwrap_or_default().into(),
    }))
}

fn file(file: &EmbeddedFile) -> io::Result<Item> {
    let mut bytes = Vec::new();
    file.read().read_to_end(&mut bytes)?;
    Ok(Item::File {
        bytes,
        name: file.filename().into(),
    })
}

#[cfg(test)]
mod tests;
