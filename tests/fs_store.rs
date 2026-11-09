//! Runs the shared storage suite against the filesystem store.
#![cfg(not(target_arch = "wasm32"))]

mod suite;

use fileboxes::store::fs::FsStore;

#[test]
fn fs_store_passes_storage_suite() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsStore::open(dir.path()).unwrap();
    pollster::block_on(suite::run(&store));
}
