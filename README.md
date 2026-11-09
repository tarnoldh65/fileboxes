# fileboxes

Organize notes into fileboxes, sections, folders, and pages. Runs on desktop and on the web at https://tarnoldh65.github.io/fileboxes/.

## Develop

Requires Rust and the Dioxus CLI (`cargo install dioxus-cli@0.7.10 --locked`).

```
dx serve --platform desktop
dx serve --platform web
cargo test
```

Browser storage tests need `wasm-bindgen-cli` (same version as `wasm-bindgen` in `Cargo.lock`) and chromedriver:

```
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --target wasm32-unknown-unknown --no-default-features --features web --test opfs_store
```

Pushes to `main` deploy the web app to GitHub Pages.
