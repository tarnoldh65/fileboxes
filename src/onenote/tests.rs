use pollster::block_on;

use super::*;
use crate::store::fs::FsStore;

fn run(text: &str) -> Run {
    Run {
        text: text.into(),
        ..Run::default()
    }
}

fn para(runs: Vec<Run>) -> Paragraph {
    Paragraph {
        runs,
        ..Paragraph::default()
    }
}

fn text_item(text: &str) -> Item {
    Item::Paragraph(para(vec![run(text)]))
}

fn container(x: f32, y: f32, items: Vec<Item>) -> Container {
    Container { x, y, items }
}

fn section(name: &str, pages: &[&str]) -> Entry {
    Entry::Section {
        name: name.into(),
        pages: pages
            .iter()
            .map(|t| NotePage {
                title: t.to_string(),
                containers: vec![container(0.0, 0.0, vec![text_item(t)])],
            })
            .collect(),
    }
}

fn group(name: &str, entries: Vec<Entry>) -> Entry {
    Entry::Group {
        name: name.into(),
        entries,
    }
}

type Attachments = Vec<(Id, Vec<u8>)>;

/// The blocks and attachments of a single converted page.
fn blocks_of(containers: Vec<Container>) -> (Vec<Block>, Attachments) {
    let entries = vec![Entry::Section {
        name: "S".into(),
        pages: vec![NotePage {
            title: "P".into(),
            containers,
        }],
    }];
    let mut converted = convert("N", entries);
    let page = converted.filebox.sections[0].folders[0].pages.remove(0);
    (page.blocks, converted.attachments)
}

fn blocks(items: Vec<Item>) -> (Vec<Block>, Attachments) {
    blocks_of(vec![container(0.0, 0.0, items)])
}

fn html(block: &Block) -> &str {
    match &block.content {
        Content::Text { html } => html,
        other => panic!("not text: {other:?}"),
    }
}

/// Every page as `section > folder > page`.
fn paths(fb: &Filebox) -> Vec<String> {
    let mut paths = Vec::new();
    for s in &fb.sections {
        for f in &s.folders {
            for p in &f.pages {
                paths.push(format!("{} > {} > {}", s.name, f.name, p.title));
            }
        }
    }
    paths
}

#[test]
fn groups_sections_and_pages_follow_the_mapping() {
    let entries = vec![
        section("Loose A", &["A1", "A2"]),
        group(
            "Group",
            vec![
                section("Inner", &["I1"]),
                group("Sub", vec![section("Deep", &["D1"])]),
            ],
        ),
        section("Loose B", &["", "B2"]),
    ];
    let converted = convert("Notebook", entries);
    assert_eq!(converted.filebox.name.as_str(), "Notebook");
    assert_eq!(
        paths(&converted.filebox),
        [
            "Notebook > Loose A > A1",
            "Notebook > Loose A > A2",
            "Notebook > Loose B > Untitled page",
            "Notebook > Loose B > B2",
            "Group > Inner > I1",
            "Group / Sub > Deep > D1",
        ]
    );
}

#[test]
fn a_single_section_gets_one_section_and_one_folder() {
    let converted = convert("Weather", vec![section("Weather", &["Sunday"])]);
    assert_eq!(paths(&converted.filebox), ["Weather > Weather > Sunday"]);
}

#[test]
fn every_run_format_becomes_html() {
    let runs = vec![
        Run {
            bold: true,
            ..run("bold")
        },
        Run {
            italic: true,
            ..run(" italic")
        },
        Run {
            underline: true,
            ..run(" under")
        },
        Run {
            strike: true,
            ..run(" struck")
        },
        Run {
            color: Some([192, 0, 0]),
            ..run(" red")
        },
        Run {
            size: Some(16.0),
            ..run(" big")
        },
        Run {
            size: Some(11.0),
            ..run(" normal <&>")
        },
    ];
    let (blocks, _) = blocks(vec![Item::Paragraph(para(runs))]);
    assert_eq!(
        html(&blocks[0]),
        "<p><b>bold</b><i> italic</i><u> under</u><s> struck</s>\
         <span style=\"color:#c00000\"> red</span>\
         <span style=\"font-size:16pt\"> big</span> normal &lt;&amp;&gt;</p>"
    );
}

#[test]
fn headings_alignment_and_nested_lists_become_html() {
    let item = |text: &str, list, level| {
        Item::Paragraph(Paragraph {
            list: Some(list),
            level,
            ..para(vec![run(text)])
        })
    };
    let (blocks, _) = blocks(vec![
        Item::Paragraph(Paragraph {
            heading: Some(2),
            ..para(vec![run("Title")])
        }),
        Item::Paragraph(Paragraph {
            align: Some("center"),
            ..para(vec![run("middle")])
        }),
        item("one", List::Bullets, 0),
        item("one a", List::Numbers, 1),
        item("one b", List::Numbers, 1),
        item("two", List::Bullets, 0),
        item("first", List::Numbers, 0),
        text_item("after"),
    ]);
    assert_eq!(
        html(&blocks[0]),
        "<h2>Title</h2><div style=\"text-align:center\">middle</div>\
         <ul><li>one<ol><li>one a</li><li>one b</li></ol></li><li>two</li></ul>\
         <ol><li>first</li></ol><p>after</p>"
    );
}

#[test]
fn links_and_typed_urls_become_links() {
    let (blocks, _) = blocks(vec![
        Item::Paragraph(para(vec![
            run("see "),
            Run {
                link: Some("https://example.com/a?b=1&c=2".into()),
                ..run("this")
            },
        ])),
        Item::Paragraph(para(vec![
            run("From <"),
            run("https://example.com"),
            run(">"),
        ])),
    ]);
    assert_eq!(
        html(&blocks[0]),
        "<p>see <a href=\"https://example.com/a?b=1&amp;c=2\">this</a></p>\
         <p>From &lt;<a href=\"https://example.com\">https://example.com</a>&gt;</p>"
    );
}

#[test]
fn empty_paragraphs_are_dropped_and_indents_kept() {
    let (blocks, _) = blocks(vec![
        text_item(""),
        text_item("  mkdir -p x"),
        text_item(" "),
        text_item("line\u{b}break"),
        text_item(""),
    ]);
    assert_eq!(
        html(&blocks[0]),
        "<p>&nbsp;&nbsp;mkdir -p x</p><p>line<br>break</p>"
    );
    let (empty, _) = self::blocks(vec![text_item("")]);
    assert!(empty.is_empty());
}

#[test]
fn tables_keep_their_cells() {
    let cell = |texts: &[&str]| texts.iter().map(|t| para(vec![run(t)])).collect();
    let bold = vec![para(vec![Run {
        bold: true,
        ..run("Name")
    }])];
    let rows = vec![vec![bold, cell(&["Notes"])], vec![cell(&["a", "", "b"])]];
    let (blocks, _) = blocks(vec![
        text_item("before"),
        Item::Table(rows),
        text_item("after"),
    ]);
    assert_eq!(blocks.len(), 3);
    assert_eq!(html(&blocks[0]), "<p>before</p>");
    let Content::Table(table) = &blocks[1].content else {
        panic!("not a table");
    };
    assert_eq!(table.cells, [["<b>Name</b>", "Notes"], ["a<br>b", ""]]);
    assert_eq!(html(&blocks[2]), "<p>after</p>");
}

#[test]
fn images_and_files_become_attachments() {
    let (blocks, attachments) = blocks(vec![
        Item::Image {
            bytes: b"\x89PNG".to_vec(),
            caption: "A chart".into(),
        },
        Item::File {
            bytes: b"%PDF".to_vec(),
            name: "report.pdf".into(),
        },
    ]);
    let Content::Image {
        attachment: image,
        caption,
    } = &blocks[0].content
    else {
        panic!("not an image");
    };
    let Content::File {
        attachment: file,
        file_name,
    } = &blocks[1].content
    else {
        panic!("not a file");
    };
    assert_eq!(
        (caption.as_str(), file_name.as_str()),
        ("A chart", "report.pdf")
    );
    assert_eq!(
        attachments,
        [(*image, b"\x89PNG".to_vec()), (*file, b"%PDF".to_vec())]
    );
}

#[test]
fn containers_are_ordered_top_to_bottom_then_left_to_right() {
    let (blocks, _) = blocks_of(vec![
        container(5.0, 3.0, vec![text_item("bottom")]),
        container(4.0, 1.0, vec![text_item("top right")]),
        container(1.0, 1.0, vec![text_item("top left")]),
    ]);
    let texts: Vec<&str> = blocks.iter().map(html).collect();
    assert_eq!(
        texts,
        ["<p>top left</p>", "<p>top right</p>", "<p>bottom</p>"]
    );
}

#[test]
fn files_that_are_not_onenote_are_errors() {
    for name in ["x.onepkg", "x.one", "x.txt", "x"] {
        assert!(read(b"not onenote", name).is_err(), "{name}");
    }
}

#[test]
fn importing_twice_gives_two_fileboxes() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsStore::open(dir.path()).unwrap();
    let entries = vec![section("S", &["P"])];
    let a = block_on(save(&store, convert("N", entries.clone()))).unwrap();
    let b = block_on(save(&store, convert("N", entries))).unwrap();
    assert_ne!(a.id, b.id);
    assert_eq!(block_on(store.list()).unwrap().len(), 2);
}

// Local tests on OneNote files that stay out of the repo. Put the files in the
// project folder and run `cargo test -- --ignored`.

fn local(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    std::fs::read(path).expect("OneNote file in the project folder")
}

fn check_weather(converted: &Converted) {
    let weather = converted
        .filebox
        .sections
        .iter()
        .flat_map(|s| &s.folders)
        .find(|f| f.name.as_str() == "Weather")
        .expect("Weather folder");
    let titles: Vec<&str> = weather.pages.iter().map(|p| p.title.as_str()).collect();
    assert_eq!(
        titles[..5],
        ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday"]
    );
    let thursday = &weather.pages[4].blocks;
    let table = thursday
        .iter()
        .find_map(|b| match &b.content {
            Content::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    assert_eq!((table.cells.len(), table.columns()), (5, 5));
    assert_eq!(table.cells[0][0], "Abc");
    let image = thursday
        .iter()
        .find_map(|b| match b.content {
            Content::Image { attachment, .. } => Some(attachment),
            _ => None,
        })
        .expect("image");
    let (_, bytes) = converted
        .attachments
        .iter()
        .find(|(id, _)| *id == image)
        .expect("image attachment");
    assert!(bytes.starts_with(b"\x89PNG"));
}

#[test]
#[ignore = "needs Notebook_Export.onepkg in the project folder"]
fn local_notebook_converts() {
    let entries = read(&local("Notebook_Export.onepkg"), "Notebook_Export.onepkg").unwrap();
    let converted = convert("Notebook_Export", entries);
    let fb = &converted.filebox;
    assert_eq!(fb.sections.len(), 1);
    assert_eq!(fb.sections[0].name.as_str(), "Notebook_Export");
    let folders: Vec<&str> = fb.sections[0]
        .folders
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    assert_eq!(folders, ["Weather", "Odd Stuff"]);
    let odd: Vec<&str> = fb.sections[0].folders[1]
        .pages
        .iter()
        .map(|p| p.title.as_str())
        .collect();
    assert!(odd.contains(&"Kubernetes Build") && odd.contains(&"One Agent"));
    check_weather(&converted);
}

#[test]
#[ignore = "needs Weather.one in the project folder"]
fn local_section_converts() {
    let converted = convert(
        "Weather",
        read(&local("Weather.one"), "Weather.one").unwrap(),
    );
    assert_eq!(converted.filebox.sections.len(), 1);
    check_weather(&converted);
}

#[test]
#[ignore = "needs Notebook_Export.onepkg in the project folder"]
fn local_notebook_imports_into_a_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsStore::open(dir.path()).unwrap();
    let bytes = local("Notebook_Export.onepkg");
    let fb = block_on(import(&store, &bytes, "Notebook_Export.onepkg")).unwrap();
    // A restart reads the pages back from Markdown; they must say the same thing.
    let loaded = block_on(FsStore::open(dir.path()).unwrap().load(fb.id)).unwrap();
    let markdown = |fb: &Filebox| -> Vec<String> {
        fb.pages()
            .map(|p| crate::markdown::to_markdown(&p.blocks))
            .collect()
    };
    assert_eq!(paths(&loaded), paths(&fb));
    assert_eq!(markdown(&loaded), markdown(&fb));
    for attachment in fb.attachments() {
        assert!(block_on(store.get_attachment(fb.id, attachment)).is_ok());
    }
}
