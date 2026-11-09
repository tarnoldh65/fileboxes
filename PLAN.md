# Fileboxes - Implementation Plan

Status: APPROVED 2026-11-08.

## Stack

- Rust 1.98, edition 2024, one crate.
- Dioxus 0.7.10 (latest stable; 0.8 is still alpha), with `desktop` and `web` features
  built from the same code. Desktop runs in a native webview; web is WASM.
- `serde` / `serde_json` for storage, `uuid` for ids, `zip` for export/import,
  `ammonia` to sanitize rich-text HTML, `rfd` for native file dialogs (desktop only),
  `web-sys` for browser storage and file upload/download (web only).
- Dev only: `tempfile` for filesystem tests.

## Data Model

```
Filebox  { id, name, sections: Vec<Section> }
Section  { id, name, folders: Vec<Folder> }
Folder   { id, name, pages: Vec<Page> }
Page     { id, title, blocks: Vec<Block> }
Block    { id, half_width, content: Content }
Content  = Text { html } | Table { rows: Vec<Vec<String>> }
         | Image { attachment, caption } | File { attachment, file_name }
         | Code { text }
```

- Text blocks hold sanitized HTML: bold, italic, underline, size, color, alignment,
  bullets and numbered lists, and links. Code is its own block type (monospace,
  whitespace preserved).
- Pages are a vertical list of blocks. Blocks can be added, deleted, and moved up or
  down, and each block can be full or half width, so two blocks can sit side by side.
- Attachments (images and files) are stored by id beside the filebox JSON, not inline.

## Storage

One `Store` trait with two backends that use the same layout:

```
<root>/<filebox-id>/filebox.json
<root>/<filebox-id>/attachments/<attachment-id>
```

- Desktop: a folder on disk (default `Documents/Fileboxes`, changeable). Pointing it at a
  OneDrive or Google Drive sync folder provides cloud storage with no API code.
- Web: the browser's Origin Private File System (OPFS).
- Export: one `.filebox` zip holding `filebox.json` plus `attachments/`. Import unzips it
  into the store; if that id already exists, it is imported as a copy with a new id.
- Saves happen automatically 1 second after the last edit.

## UI Layout

```
+------------------------------------------------------------+
| [Filebox v]  | Section A | Section B | + |                 |
+------+-------+---------------------------------------------+
|Folder| Pages | Page title                                  |
| tab  | list  | [B][I][U][size][color][align][list][link]   |
|Folder|       | blocks...                                   |
| tab  |       |                                             |
|  +   |   +   |                                             |
+------+-------+---------------------------------------------+
```

- Filebox dropdown includes New, Rename, Delete, Export, and Import.
- Sections, folders, and pages are created with `+`, renamed by double-click, and
  deleted from a right-click menu after a confirmation.

## Testing

- All model operations, serialization, sanitization, storage layout, and export/import
  live in plain Rust modules with no UI code and are unit tested.
- The filesystem backend is tested against `tempfile` directories.
- The UI is tested by rendering components to HTML with `dioxus-ssr` and checking
  structure (tabs, list, dropdown present with the right names).
- CPU limit: `.cargo/config.toml` sets `[build] jobs = 2` and
  `[env] RUST_TEST_THREADS = "2"`, so builds and tests use at most 2 cores. CI uses the
  same file.

## Phases

Every phase ends with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
and `cargo test` (both feature sets) passing, plus one commit.

### Phase 1 - Scaffolding

- [x] Crate with Dioxus 0.7.10, `desktop` (default) and `web` features, `Dioxus.toml`.
- [x] `.gitignore`: keep the existing Rust entries and add `dist/` and `.dioxus/`.
- [x] `.cargo/config.toml` with the CPU limits above.
- [x] Module skeleton: `model`, `store` (`fs`, `opfs`), `archive`, `sanitize`, `ui`.
- [x] GitHub Actions `ci.yml`: fmt, clippy, and tests on push and pull request.
- [x] README: one line on what it is, plus build, run, and test commands.

Success criteria:
- [x] `dx serve --platform desktop` and `dx serve --platform web` both show an empty shell.
- [x] `cargo test` runs a placeholder test with 2 test threads.
- [x] CI passes on GitHub.

### Phase 2 - Data model

- [x] Structs and `Block` enum per the Data Model section.
- [x] Operations: create, rename, delete, and reorder at every level; move a page to
      another folder; move a folder to another section.

Success criteria (unit tests):
- [x] Each operation changes only its target (siblings and order are preserved).
- [x] Deleting a section removes its folders and pages; ids stay unique.
- [x] JSON round-trip of a filebox with every block type is lossless.
- [x] Empty names are rejected; nothing else is validated.

### Phase 3 - Storage

- [x] `Store` trait: list, load, save, delete filebox; put and get attachment.
- [x] Filesystem backend, plus a small config file that remembers the root folder.
- [x] Saving deletes attachments no block references, so deleting a block frees its
      attachment.

Success criteria (unit tests):
- [x] Save, then load, returns an equal filebox; on-disk layout matches the spec.
- [x] Attachments round-trip byte for byte; unused ones are removed on save.
- [x] Listing returns every filebox in the root folder by name.

### Phase 4 - Navigation UI

- [x] Filebox dropdown, section tabs on top, folder tabs on the side, page list panel.
- [x] Create, rename, and delete at every level from the UI; selection persists while
      switching between fileboxes.
- [x] Debounced autosave (1 second after the last edit).
- [x] Desktop: choose the storage folder with a folder picker, saved to the config file.

Success criteria:
- [x] SSR tests: the rendered shell has the dropdown with all filebox names, one tab per
      section and folder, and one list item per page, in order.
- [x] Manual: create a filebox with 2 sections, 3 folders, and 5 pages; restart; all of
      it is still there.

### Phase 5 - Page editing

- [x] Text block: `contenteditable` with a toolbar (bold, italic, underline, size, color,
      alignment, bullets, numbered list, link). HTML is sanitized with
      `ammonia` on save.
- [x] Table block: add/remove rows and columns, edit cells.
- [x] Image block: pick a file, display it inline, edit the caption.
- [x] File block: pick any file; click to open (desktop) or download (web).
- [x] Code block: monospace text area that preserves whitespace.
- [x] Add, delete, and move blocks up and down; toggle full or half width.
- [x] Desktop: save immediately when the window closes, so recent edits are not lost.

Success criteria:
- [x] Unit tests: the sanitizer keeps every supported format and strips `<script>`,
      event handlers, and `javascript:` links; table row/column edits keep the grid
      rectangular.
- [x] Manual: a page with every block type and every text format survives a restart
      unchanged.

### Phase 6 - Export and import

- [x] Export a filebox to `.filebox` (zip); import from `.filebox`.

Success criteria (unit tests):
- [x] Export, then import into an empty store, gives an equal filebox and identical
      attachment bytes.
- [x] Importing over an existing id creates a copy with a new id and the same content.
- [x] Manual (desktop): export a filebox with attachments, import it back, and the copy
      opens with the same pages, images, and files.

### Phase 7 - Web build and GitHub Pages

- [x] OPFS backend implementing `Store`.
- [x] Browser export (download) and import (file upload).
- [x] `deploy.yml`: on push to `main`, build with `dx bundle --platform web --release`
      using base path `/fileboxes`, then deploy to GitHub Pages.

Success criteria:
- [x] Pushing to `main` publishes the app at `https://tarnoldh65.github.io/fileboxes/`.
- [ ] Manual: in Chrome, Edge, and Firefox, data persists across reloads, and
      export/import works.
- [ ] Manual: an export from desktop imports on web, and the reverse.
- [x] The OPFS backend passes the same storage test suite (run with
      `wasm-bindgen-test` in headless Chrome in CI).

### Phase 8 - Finish

- [ ] Final README pass (build, run, test, deploy, where data is stored).
- [ ] Remove anything unused.

Success criteria:
- [ ] Every checkbox above is checked, and CI and deploy are green on `main`.
