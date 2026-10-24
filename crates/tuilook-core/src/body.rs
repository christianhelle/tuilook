use tuilook_model::{Body, BodyType};

/// Width html2text wraps at; the reading pane re-wraps to fit.
const WRAP_WIDTH: usize = 1000;

/// A terminal cannot show images, and their alt text is mostly noise.
const HIDE_IMAGES: &str = "img, svg, picture { display: none; }";

/// Renders a message body as plain text, numbering links as footnotes.
pub fn body_text(body: &Body) -> String {
    match body.content_type {
        BodyType::Text => body.content.clone(),
        BodyType::Html => html_to_text(&body.content)
            // Malformed HTML still deserves to be readable.
            .unwrap_or_else(|_| body.content.clone()),
    }
}

fn html_to_text(html: &str) -> Result<String, html2text::Error> {
    html2text::config::plain()
        .link_footnotes(true)
        .add_agent_css(HIDE_IMAGES)?
        .string_from_read(html.as_bytes(), WRAP_WIDTH)
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

    #[test]
    fn images_are_left_out_even_with_alt_text() {
        let text = body_text(&html(
            r#"<p>Hi</p><img src="https://cdn.example.com/logo.png" alt="Company logo"><p>Bye</p>"#,
        ));

        assert_eq!(text, "Hi\n\nBye\n");
    }
}
