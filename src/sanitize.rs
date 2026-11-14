//! Rich-text HTML sanitizing.

/// The style properties the editor's toolbar produces.
const STYLES: [&str; 3] = ["color", "font-size", "text-align"];

/// Keeps only supported properties, in compact `property:value` form. `None` drops the
/// attribute when nothing is left.
fn filter_style(style: &str) -> Option<String> {
    let kept: Vec<String> = style
        .split(';')
        .filter_map(|declaration| {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            STYLES
                .contains(&property.as_str())
                .then(|| format!("{property}:{}", value.trim()))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

/// Keeps formatting markup (bold, italic, underline, lists, links, and the color, size,
/// and alignment styles the editor produces) and strips scripts, event handlers, and
/// unsafe URLs.
pub fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["style"])
        .attribute_filter(|_, attribute, value| match attribute {
            "style" => filter_style(value).map(Into::into),
            _ => Some(value.into()),
        })
        // Links do not navigate inside the editor, and no `rel` keeps them plain Markdown.
        .link_rel(None)
        .clean(html)
        .to_string()
}

/// Escapes plain text so it displays as typed when used as HTML.
pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_supported_formatting() {
        for html in [
            "<b>bold</b>",
            "<i>italic</i>",
            "<u>underline</u>",
            "<ul><li>one</li></ul>",
            "<ol><li>one</li></ol>",
            "<p>para<br></p>",
        ] {
            assert_eq!(sanitize(html), html);
        }
    }

    #[test]
    fn keeps_supported_styles_in_compact_form() {
        for (html, expected) in [
            (
                "<div style=\"text-align: center;\">centered</div>",
                "<div style=\"text-align:center\">centered</div>",
            ),
            (
                "<span style=\"color: rgb(204, 0, 0);\">red</span>",
                "<span style=\"color:rgb(204, 0, 0)\">red</span>",
            ),
            (
                "<span style=\"font-size: x-large;\">big</span>",
                "<span style=\"font-size:x-large\">big</span>",
            ),
        ] {
            assert_eq!(sanitize(html), expected);
        }
    }

    #[test]
    fn keeps_safe_links() {
        assert_eq!(
            sanitize("<a href=\"https://example.com\">x</a>"),
            "<a href=\"https://example.com\">x</a>"
        );
    }

    #[test]
    fn strips_scripts_handlers_and_javascript_links() {
        assert_eq!(sanitize("a<script>alert(1)</script>b"), "ab");
        assert_eq!(
            sanitize("<b onclick=\"alert(1)\">x</b><img src=\"x.png\" onerror=\"alert(1)\">"),
            "<b>x</b><img src=\"x.png\">"
        );
        assert_eq!(
            sanitize("<a href=\"javascript:alert(1)\">x</a>"),
            "<a>x</a>"
        );
    }

    #[test]
    fn strips_unsupported_style_properties() {
        assert_eq!(
            sanitize("<span style=\"color: red; position: fixed; background: url(x)\">x</span>"),
            "<span style=\"color:red\">x</span>"
        );
    }

    #[test]
    fn escape_makes_text_safe_html() {
        assert_eq!(
            escape("a < b & \"c\" > d"),
            "a &lt; b &amp; &quot;c&quot; &gt; d"
        );
        assert_eq!(
            sanitize(&escape("<script>x</script>")),
            "&lt;script&gt;x&lt;/script&gt;"
        );
    }

    #[test]
    fn strips_empty_styles_and_old_link_rel() {
        assert_eq!(
            sanitize("<b style=\"\">x</b><i style=\"font-weight: bold\">y</i>"),
            "<b>x</b><i>y</i>"
        );
        assert_eq!(
            sanitize("<a href=\"https://e.com\" rel=\"noopener noreferrer\">z</a>"),
            "<a href=\"https://e.com\">z</a>"
        );
        assert_eq!(sanitize("a style=\"\" b"), "a style=\"\" b");
    }
}
