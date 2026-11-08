use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");

#[component]
pub fn App() -> Element {
    rsx! {
        document::Title { "Fileboxes" }
        document::Stylesheet { href: MAIN_CSS }
        Shell {}
    }
}

#[component]
fn Shell() -> Element {
    rsx! {
        header { class: "topbar",
            select { class: "filebox-select" }
            nav { class: "sections" }
        }
        main { class: "body",
            nav { class: "folders" }
            ul { class: "pages" }
            article { class: "page" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_renders_layout_regions() {
        let mut dom = VirtualDom::new(Shell);
        dom.rebuild_in_place();
        let html = dioxus_ssr::render(&dom);
        for class in ["filebox-select", "sections", "folders", "pages", "page"] {
            assert!(
                html.contains(&format!("class=\"{class}\"")),
                "missing {class}"
            );
        }
    }
}
