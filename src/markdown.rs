//! Page content as Markdown, the format pages are stored in. Each block starts with a
//! `<!-- block -->` or `<!-- block half -->` comment. Formatting Markdown cannot express
//! (color, size, alignment, underline) stays as inline HTML.

use htmd::element_handler::{HandlerResult, Handlers};
use htmd::options::{BrStyle, BulletListMarker, Options, TranslationMode};
use htmd::{Element, HtmlToMarkdown};
use pulldown_cmark::{Event, Parser, Tag, TagEnd, html};

use crate::model::{Block, Content, Id, Table};
use crate::sanitize::sanitize;

const BLOCK: &str = "<!-- block -->";
const HALF: &str = "<!-- block half -->";
/// Image and file links point here, relative to `pages/<page-id>.md`.
const ATTACHMENTS: &str = "../attachments/";

/// Plain `<p>`s and `<div>`s (what the editor makes on Enter) become paragraphs; styled
/// ones stay HTML.
fn paragraph_handler(handlers: &dyn Handlers, element: Element) -> Option<HandlerResult> {
    if !element.attrs.is_empty() {
        return handlers.fallback(element);
    }
    let content = handlers.walk_children(element.node).content;
    let lines: Vec<String> = content
        .trim_matches('\n')
        .lines()
        .map(escape_line_start)
        .collect();
    Some(format!("\n\n{}\n\n", lines.join("\n")).into())
}

/// Escapes the line starts htmd leaves that Markdown reads as a rule (`---`) or a
/// numbered list (`1)`).
fn escape_line_start(line: &str) -> String {
    if line.starts_with("---") && line.chars().all(|c| c == '-' || c == ' ') {
        return format!("\\{line}");
    }
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if (1..=9).contains(&digits) && line[digits..].starts_with(')') {
        return format!("{}\\{}", &line[..digits], &line[digits..]);
    }
    line.into()
}

fn html_to_markdown(html: &str) -> String {
    HtmlToMarkdown::builder()
        .options(Options {
            translation_mode: TranslationMode::Faithful,
            bullet_list_marker: BulletListMarker::Dash,
            ul_bullet_spacing: 1,
            ol_number_spacing: 1,
            br_style: BrStyle::Backslash,
            ..Default::default()
        })
        .add_handler(vec!["p", "div"], paragraph_handler)
        .build()
        .convert(html)
        .expect("HTML always parses")
}

fn escape_label(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

/// A table cell on one line: Markdown when it fits, otherwise the cell's HTML.
fn cell_markdown(html: &str) -> String {
    let md = html_to_markdown(html);
    let cell = if md.contains('\n') {
        html.replace('\n', "")
    } else {
        md
    };
    cell.replace('|', "\\|")
}

fn table_markdown(table: &Table) -> String {
    let row = |cells: &Vec<String>| {
        let cells: Vec<_> = cells.iter().map(|c| cell_markdown(c)).collect();
        format!("| {} |\n", cells.join(" | "))
    };
    let mut md = row(&table.cells[0]);
    md.push_str(&format!("|{}\n", " --- |".repeat(table.columns())));
    for cells in &table.cells[1..] {
        md.push_str(&row(cells));
    }
    md
}

fn code_markdown(text: &str) -> String {
    let longest_run = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest_run.max(2) + 1);
    format!("{fence}\n{text}\n{fence}")
}

pub fn to_markdown(blocks: &[Block]) -> String {
    let mut md = String::new();
    for block in blocks {
        md.push_str(if block.half_width { HALF } else { BLOCK });
        md.push('\n');
        let body = match &block.content {
            // Sanitized again so text saved by older versions is normalized too.
            Content::Text { html } => html_to_markdown(&sanitize(html)),
            Content::Table(table) => table_markdown(table),
            Content::Image {
                attachment,
                caption,
            } => format!("![{}]({ATTACHMENTS}{attachment})", escape_label(caption)),
            Content::File {
                attachment,
                file_name,
            } => format!("[{}]({ATTACHMENTS}{attachment})", escape_label(file_name)),
            Content::Code { text } => code_markdown(text),
        };
        md.push_str(body.trim_end());
        md.push_str("\n\n");
    }
    md
}

/// The fence a line opens or closes: a run of 3 or more backticks or tildes.
fn fence(line: &str) -> Option<&str> {
    let line = line.trim();
    let mark = line.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let run = &line[..line.len() - line.trim_start_matches(mark).len()];
    (run.len() >= 3).then_some(run)
}

/// Splits a page into `(half_width, markdown)` segments at block comments. Text before
/// the first comment (or a file with none) is one full-width segment. Comments inside
/// code fences are code, not separators.
fn segments(md: &str) -> Vec<(bool, String)> {
    let mut segments = vec![(false, String::new())];
    let mut open_fence: Option<&str> = None;
    for line in md.lines() {
        let trimmed = line.trim();
        match (open_fence, fence(line)) {
            (None, Some(run)) => open_fence = Some(run),
            // A closing fence is only fence characters, at least as many as opened it.
            (Some(open), Some(run))
                if run.starts_with(&open[..1]) && run.len() >= open.len() && run == trimmed =>
            {
                open_fence = None
            }
            _ => {}
        }
        if open_fence.is_none() && (trimmed == BLOCK || trimmed == HALF) {
            segments.push((trimmed == HALF, String::new()));
        } else {
            let text = &mut segments.last_mut().expect("never empty").1;
            text.push_str(line);
            text.push('\n');
        }
    }
    if segments[0].1.trim().is_empty() {
        segments.remove(0);
    }
    segments
}

/// True if `events` is exactly one element: its first `Start` closes at the last event.
fn single(events: &[Event]) -> bool {
    let mut depth = 0;
    for (i, event) in events.iter().enumerate() {
        match event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return i == events.len() - 1;
        }
    }
    false
}

fn text_of(events: &[Event]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Text(t) | Event::Code(t) => Some(t.as_ref()),
            _ => None,
        })
        .collect()
}

fn attachment_id(url: &str) -> Option<Id> {
    url.strip_prefix(ATTACHMENTS)?.parse().ok()
}

fn parse_table(events: &[Event]) -> Table {
    let mut cells: Vec<Vec<String>> = Vec::new();
    let mut cell: Option<Vec<Event>> = None;
    for event in events {
        match event {
            Event::Start(Tag::TableHead | Tag::TableRow) => cells.push(Vec::new()),
            Event::Start(Tag::TableCell) => cell = Some(Vec::new()),
            Event::End(TagEnd::TableCell) => {
                let mut html = String::new();
                html::push_html(&mut html, cell.take().expect("cell started").into_iter());
                cells.last_mut().expect("row started").push(sanitize(&html));
            }
            event => {
                if let Some(cell) = &mut cell {
                    cell.push(event.clone());
                }
            }
        }
    }
    let columns = cells[0].len();
    for row in &mut cells {
        row.resize(columns, String::new());
    }
    Table { cells }
}

fn parse_block(md: &str) -> Content {
    let events: Vec<Event> = Parser::new_ext(md, pulldown_cmark::Options::ENABLE_TABLES).collect();
    if single(&events) {
        match &events[0] {
            Event::Start(Tag::Table(_)) => return Content::Table(parse_table(&events)),
            Event::Start(Tag::CodeBlock(_)) => {
                let text = text_of(&events);
                let text = text.strip_suffix('\n').unwrap_or(&text);
                return Content::Code { text: text.into() };
            }
            Event::Start(Tag::Paragraph) if single(&events[1..events.len() - 1]) => {
                let inner = &events[1..events.len() - 1];
                match &inner[0] {
                    Event::Start(Tag::Image { dest_url, .. }) => {
                        if let Some(attachment) = attachment_id(dest_url) {
                            let caption = text_of(inner);
                            return Content::Image {
                                attachment,
                                caption,
                            };
                        }
                    }
                    Event::Start(Tag::Link { dest_url, .. }) => {
                        if let Some(attachment) = attachment_id(dest_url) {
                            let file_name = text_of(inner);
                            return Content::File {
                                attachment,
                                file_name,
                            };
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let mut html = String::new();
    html::push_html(&mut html, events.into_iter());
    Content::Text {
        html: sanitize(html.trim_end()),
    }
}

pub fn from_markdown(md: &str) -> Vec<Block> {
    segments(md)
        .into_iter()
        .map(|(half_width, md)| Block {
            half_width,
            ..Block::new(parse_block(&md))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn text(html: &str) -> Block {
        Block::new(Content::Text { html: html.into() })
    }

    fn contents(blocks: &[Block]) -> Vec<(bool, Content)> {
        blocks
            .iter()
            .map(|b| (b.half_width, b.content.clone()))
            .collect()
    }

    /// Every block type, with every toolbar format in text and table cells.
    fn every_block() -> Vec<Block> {
        let (image, file) = (Uuid::new_v4(), Uuid::new_v4());
        let mut table = Table::default();
        table.add_column();
        table.set(0, 0, "Name");
        table.set(0, 1, "<b>Notes</b>");
        table.set(1, 0, "<span style=\"color:#c00000\">red</span> a|b");
        table.set(1, 1, "<ul><li>one</li><li>two</li></ul>");
        table.set(1, 2, "first<br>second");
        let mut code = Block::new(Content::Code {
            text: "let x = `1`;\n```\n<!-- block -->\n".into(),
        });
        code.half_width = true;
        vec![
            text(
                "<p><b>Bold</b> <i>italic</i> <u>under</u> \
                 <span style=\"font-size:x-large\">big</span> \
                 <span style=\"color:#0050c0\">blue</span> \
                 <a href=\"https://example.com\">link</a></p>\
                 <div style=\"text-align:center\">centered <b>bold</b></div>\
                 <ul><li>bullet</li></ul><ol><li>number</li></ol>\
                 <div>line two</div>",
            ),
            Block::new(Content::Table(table)),
            Block::new(Content::Image {
                attachment: image,
                caption: "A [caption]".into(),
            }),
            Block::new(Content::File {
                attachment: file,
                file_name: "report.pdf".into(),
            }),
            code,
            text(""),
        ]
    }

    #[test]
    fn markdown_reads_as_markdown() {
        let md = to_markdown(&every_block());
        for expected in [
            "<!-- block -->\n**Bold** *italic* <u>under</u>",
            "<span style=\"font-size:x-large\">big</span>",
            "[link](https://example.com)",
            "<div style=\"text-align:center\">centered <b>bold</b></div>",
            "- bullet",
            "1. number",
            "\n\nline two\n",
            "| Name | **Notes** |  |\n| --- | --- | --- |\n",
            "| <span style=\"color:#c00000\">red</span> a\\|b | <ul><li>one</li><li>two</li></ul> | first<br>second |",
            "![A \\[caption\\]](../attachments/",
            "[report.pdf](../attachments/",
            "<!-- block half -->\n````\nlet x = `1`;\n```\n<!-- block -->\n\n````",
        ] {
            assert!(md.contains(expected), "missing {expected:?} in\n{md}");
        }
    }

    #[test]
    fn every_block_and_format_survives_a_round_trip() {
        let blocks = every_block();
        let back = from_markdown(&to_markdown(&blocks));
        assert_eq!(back.len(), blocks.len());
        assert_eq!(contents(&back[2..]), contents(&blocks[2..]));

        let Content::Text { html } = &back[0].content else {
            panic!("not text");
        };
        for tag in [
            "<strong>Bold</strong>",
            "<em>italic</em>",
            "<u>under</u>",
            "<span style=\"font-size:x-large\">big</span>",
            "<span style=\"color:#0050c0\">blue</span>",
            "<a href=\"https://example.com\">link</a>",
            "<div style=\"text-align:center\">centered <b>bold</b></div>",
            "<ul>\n<li>bullet</li>\n</ul>",
            "<ol>\n<li>number</li>\n</ol>",
            "<p>line two</p>",
        ] {
            assert!(html.contains(tag), "missing {tag:?} in {html}");
        }

        let Content::Table(table) = &back[1].content else {
            panic!("not a table");
        };
        assert_eq!(table.cells[0], ["Name", "<strong>Notes</strong>", ""]);
        assert_eq!(
            table.cells[1],
            [
                "<span style=\"color:#c00000\">red</span> a|b",
                "<ul><li>one</li><li>two</li></ul>",
                "first<br>second",
            ]
        );
    }

    #[test]
    fn converting_twice_gives_the_same_markdown() {
        let md = to_markdown(&every_block());
        assert_eq!(to_markdown(&from_markdown(&md)), md);
    }

    #[test]
    fn file_without_block_comments_is_one_text_block() {
        let blocks = from_markdown("# Title\n\nSome *text*.\n\n| a | b |\n| - | - |\n| 1 | 2 |\n");
        assert_eq!(blocks.len(), 1);
        let Content::Text { html } = &blocks[0].content else {
            panic!("not text");
        };
        assert!(html.starts_with("<h1>Title</h1>"));
        assert!(html.contains("<em>text</em>") && html.contains("<table>"));
    }

    #[test]
    fn text_before_the_first_comment_is_kept() {
        let blocks = from_markdown("intro\n<!-- block half -->\nmore\n");
        assert_eq!(
            contents(&blocks),
            [
                (
                    false,
                    Content::Text {
                        html: "<p>intro</p>".into()
                    }
                ),
                (
                    true,
                    Content::Text {
                        html: "<p>more</p>".into()
                    }
                ),
            ]
        );
    }

    #[test]
    fn short_table_rows_are_padded() {
        let blocks = from_markdown("<!-- block -->\n| a | b |\n| - | - |\n| 1 |\n");
        let Content::Table(table) = &blocks[0].content else {
            panic!("not a table");
        };
        assert_eq!(table.cells, [["a", "b"], ["1", ""]]);
    }

    #[test]
    fn links_elsewhere_stay_text() {
        let blocks = from_markdown("<!-- block -->\n[site](https://example.com)\n");
        assert!(matches!(blocks[0].content, Content::Text { .. }));
    }

    #[test]
    fn unsafe_markdown_html_is_stripped() {
        let blocks = from_markdown(
            "<!-- block -->\n<script>alert(1)</script>\n\n[x](javascript:alert(1)) \
             <img src=\"x.png\" onerror=\"alert(1)\">\n",
        );
        let Content::Text { html } = &blocks[0].content else {
            panic!("not text");
        };
        assert!(
            !html.contains("script") && !html.contains("javascript") && !html.contains("onerror")
        );
    }

    #[test]
    fn editing_the_markdown_changes_the_blocks() {
        let mut table = Table::default();
        table.set(1, 0, "old");
        let md = to_markdown(&[
            text("<p><b>Bold</b> words</p>"),
            Block::new(Content::Table(table)),
        ]);
        let md = md
            .replace("**Bold**", "*Bold*")
            .replace("| old |", "| **new** |")
            + "<!-- block half -->
```
x = 1
```
";

        let mut expected = Table::default();
        expected.set(1, 0, "<strong>new</strong>");
        let mut code = Block::new(Content::Code {
            text: "x = 1".into(),
        });
        code.half_width = true;
        assert_eq!(
            from_markdown(&md),
            [
                text("<p><em>Bold</em> words</p>"),
                Block::new(Content::Table(expected)),
                code
            ]
        );
    }

    #[test]
    fn text_that_looks_like_a_rule_or_numbered_list_stays_text() {
        for html in [
            "<p>---</p>",
            "<p>- - -</p>",
            "<p>1) one</p>",
            "<div>2) two</div>",
        ] {
            let back = from_markdown(&to_markdown(&[text(html)]));
            let Content::Text { html: back } = &back[0].content else {
                panic!("not text");
            };
            assert!(back.starts_with("<p>"), "{html} became {back}");
        }
    }
}
