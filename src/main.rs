mod archive;
mod model;
mod sanitize;
mod store;
mod ui;

fn main() {
    dioxus::launch(ui::App);
}
