use crate::model::{Body, BodyType};

/// Width html2text wraps at; the reading pane re-wraps to fit.
const WRAP_WIDTH: usize = 1000;

/// A terminal cannot show images, and their alt text is mostly noise.
const HIDE_IMAGES: &str = "img, svg, picture { display: none; }";

/// Lines of at least this many repeated separator characters are shortened.
const MIN_SEPARATOR_LEN: usize = 10;

/// Renders a message body as plain text, numbering links as footnotes.
pub fn body_text(body: &Body) -> String {
    let text = match body.content_type {
        BodyType::Text => body.content.clone(),
        BodyType::Html => html_to_text(&body.content)
            // Malformed HTML still deserves to be readable.
            .unwrap_or_else(|_| body.content.clone()),
    };
    tidy_lines(&text)
}

/// Longer runs of spaces are table-column padding, not layout worth keeping.
const MAX_SPACE_RUN: usize = 8;

/// Shortens separator rows, column padding and runs of blank lines, all of
/// which turn into screenfuls of nothing in a narrow pane.
fn tidy_lines(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        if is_blank(line) {
            // Skip leading blanks and any blank after another.
            if lines.last().is_none_or(|last| last.is_empty()) {
                continue;
            }
            lines.push(String::new());
        } else if is_separator(line.trim()) {
            lines.push("───".to_owned());
        } else {
            lines.push(shrink_padding(line));
        }
    }
    if lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

/// Drops long runs of spaces at either end of a line and narrows them in
/// between to two spaces, so columns stay apart. Short runs are kept, like
/// the trailing space of a `-- ` signature line.
fn shrink_padding(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut run = String::new();
    for c in line.chars() {
        if c.is_whitespace() {
            run.push(c);
            continue;
        }
        if run.chars().count() <= MAX_SPACE_RUN {
            out.push_str(&run);
        } else if !out.is_empty() {
            out.push_str("  ");
        }
        run.clear();
        out.push(c);
    }
    if run.chars().count() <= MAX_SPACE_RUN {
        out.push_str(&run);
    }
    out
}

/// Whitespace, or characters that render as nothing.
fn is_blank(line: &str) -> bool {
    line.chars().all(|c| {
        c.is_whitespace()
            || matches!(
                c,
                '\u{ad}' | '\u{34f}' | '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}'
            )
    })
}

fn is_separator(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    "-=_*~#─━═".contains(first)
        && chars.all(|c| c == first)
        && line.chars().count() >= MIN_SEPARATOR_LEN
}

fn html_to_text(html: &str) -> Result<String, html2text::Error> {
    html2text::config::plain()
        .link_footnotes(true)
        // Mail uses tables for layout; their borders are just rows of dashes.
        .no_table_borders()
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
            "See [the docs][1].\n\nThanks\n\n[1]: https://example.com/docs"
        );
    }

    #[test]
    fn images_are_left_out_even_with_alt_text() {
        let text = body_text(&html(
            r#"<p>Hi</p><img src="https://cdn.example.com/logo.png" alt="Company logo"><p>Bye</p>"#,
        ));

        assert_eq!(text, "Hi\n\nBye");
    }

    fn text(content: &str) -> Body {
        Body {
            content_type: BodyType::Text,
            content: content.to_owned(),
        }
    }

    #[test]
    fn runs_of_blank_lines_become_one() {
        // Newsletters pad with invisible characters, not just spaces.
        let body =
            text("Hello\n\n\n   \n\u{a0}\n\t\n\u{200c}\u{a0}\u{34f}\u{ad}\u{feff}\nWorld\n\nBye");

        assert_eq!(body_text(&body), "Hello\n\nWorld\n\nBye");
    }

    #[test]
    fn column_padding_shrinks_so_it_does_not_wrap_into_empty_rows() {
        let body = text(&format!(
            "Order{}#42{}\n{}Total  $9",
            " ".repeat(500),
            " ".repeat(300),
            "\u{a0}".repeat(400)
        ));

        assert_eq!(body_text(&body), "Order  #42\nTotal  $9");
    }

    #[test]
    fn short_indentation_and_spacing_are_kept() {
        let body = text("    indented code\n> quoted  reply\ntwo  spaces");

        assert_eq!(
            body_text(&body),
            "    indented code\n> quoted  reply\ntwo  spaces"
        );
    }

    #[test]
    fn blank_lines_at_the_start_and_end_are_dropped() {
        let body = text("\n \n\u{a0}\nHello\n\nWorld\n\n\n");

        assert_eq!(body_text(&body), "Hello\n\nWorld");
    }

    #[test]
    fn empty_layout_cells_leave_no_gaps() {
        let text = body_text(&html(
            "<table><tr><td>&nbsp;</td></tr><tr><td><p>Hi</p></td></tr>\
             <tr><td>&nbsp;</td></tr><tr><td>&nbsp;</td></tr><tr><td><p>Bye</p></td></tr></table>",
        ));

        assert!(!text.contains("\n\n\n"), "{text:?}");
    }

    #[test]
    fn long_separator_lines_are_shortened() {
        let body = text(&format!(
            "Hello\n{}\n  {}  \nBye\n{}",
            "-".repeat(80),
            "=".repeat(40),
            "_".repeat(60)
        ));

        assert_eq!(body_text(&body), "Hello\n───\n───\nBye\n───");
    }

    #[test]
    fn separators_typed_into_html_are_shortened() {
        let text = body_text(&html(&format!(
            "<p>Hi</p><p>{}</p><p>Bye</p>",
            "-".repeat(70)
        )));

        assert_eq!(text, "Hi\n\n───\n\nBye");
    }

    #[test]
    fn dashes_inside_text_are_kept() {
        let body = text("-- \nSigned -- me\n---\n2026-10-25");

        assert_eq!(body_text(&body), "-- \nSigned -- me\n---\n2026-10-25");
    }

    #[test]
    fn a_horizontal_rule_adds_no_dashes() {
        let text = body_text(&html("<p>Above</p><hr><p>Below</p>"));

        assert_eq!(text, "Above\n\nBelow");
    }

    #[test]
    fn layout_tables_have_no_borders() {
        let text = body_text(&html(
            "<table><tr><td>Order</td><td>#42</td></tr><tr><td>Total</td><td>$9</td></tr></table>",
        ));

        assert!(!text.contains('-') && !text.contains('─'), "{text}");
        assert!(text.contains("Order") && text.contains("$9"), "{text}");
    }

    #[test]
    fn links_around_only_an_image_are_left_out_too() {
        let text = body_text(&html(
            r#"<p><a href="https://shop.example.com/sale"><img src="banner.png" alt="Sale!"></a></p>
               <p>Read <a href="https://example.com/post">the post</a>.</p>"#,
        ));

        assert_eq!(text, "Read [the post][1].\n\n[1]: https://example.com/post");
    }
}
