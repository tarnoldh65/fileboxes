//! Rich-text HTML sanitizing.

use std::collections::HashSet;

/// Keeps formatting markup (bold, italic, underline, lists, links, and the color, size,
/// and alignment styles the editor produces) and strips scripts, event handlers, and
/// unsafe URLs.
pub fn sanitize(html: &str) -> String {
    ammonia::Builder::default()
        .add_generic_attributes(["style"])
        .filter_style_properties(HashSet::from(["color", "font-size", "text-align"]))
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
            "<a href=\"https://example.com\" rel=\"noopener noreferrer\">x</a>"
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
            "<a rel=\"noopener noreferrer\">x</a>"
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
}
