//! Runs the shared storage suite against the browser store, in a headless browser:
//! `cargo test --target wasm32-unknown-unknown --no-default-features --features web --test opfs_store`
#![cfg(target_arch = "wasm32")]

mod suite;

use fileboxes::store::opfs::OpfsStore;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn opfs_store_passes_storage_suite() {
    let store = OpfsStore::new(format!("test-{}", uuid::Uuid::new_v4()));
    suite::run(&store).await;
}
