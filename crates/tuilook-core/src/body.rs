use tuilook_model::{Body, BodyType};

/// Width html2text wraps at; the reading pane re-wraps to fit.
const WRAP_WIDTH: usize = 1000;

/// Renders a message body as plain text, numbering links as footnotes.
pub fn body_text(body: &Body) -> String {
    match body.content_type {
        BodyType::Text => body.content.clone(),
        BodyType::Html => html2text::config::plain()
            .link_footnotes(true)
            .string_from_read(body.content.as_bytes(), WRAP_WIDTH)
            // Malformed HTML still deserves to be readable.
            .unwrap_or_else(|_| body.content.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(content: &str) -> Body {
        Body {
            content_type: BodyType::Html,
            content: content.to_owned(),
        }
    }

    #[test]
    fn text_bodies_are_shown_as_is() {
        let body = Body {
            content_type: BodyType::Text,
            content: "Hello\n\n<not html>".to_owned(),
        };

        assert_eq!(body_text(&body), "Hello\n\n<not html>");
    }

    #[test]
    fn html_links_become_numbered_footnotes() {
        let text = body_text(&html(
            r#"<p>See <a href="https://example.com/docs">the docs</a>.</p><p>Thanks</p>"#,
        ));

        assert_eq!(
            text,
            "See [the docs][1].\n\nThanks\n\n[1]: https://example.com/docs\n"
        );
    }
}
