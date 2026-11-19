//! Word wrapping, so long lines in a message being written fold instead of
//! running off the side of the body.

use ratatui::text::Line;

/// One row a logical line is wrapped into, and where in that line it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The characters of the logical line before this row.
    pub start: usize,
    pub text: String,
}

/// Lines of text wrapped to a width, as the body shows them.
#[derive(Debug)]
pub struct Wrapped {
    /// Each logical line's rows, in order.
    lines: Vec<Vec<Row>>,
}

impl Wrapped {
    /// `lines` wrapped to `width` columns.
    pub fn new(lines: &[String], width: usize) -> Self {
        let width = width.max(1);
        Wrapped {
            lines: lines.iter().map(|line| wrap(line, width)).collect(),
        }
    }

    /// The rows of logical line `line`.
    pub fn line_rows(&self, line: usize) -> &[Row] {
        self.lines.get(line).map_or(&[], Vec::as_slice)
    }

    /// Rows logical line `line` takes.
    pub fn height(&self, line: usize) -> usize {
        self.line_rows(line).len().max(1)
    }

    /// The first row logical line `line` is shown on.
    pub fn top_of(&self, line: usize) -> usize {
        self.lines.iter().take(line).map(Vec::len).sum()
    }

    /// Rows every line takes together.
    pub fn total(&self) -> usize {
        self.lines.iter().map(Vec::len).sum()
    }

    /// Where a place in logical line `line` is shown: the row, and the
    /// columns from its start.
    pub fn place(&self, line: usize, column: usize) -> (usize, usize) {
        let rows = self.line_rows(line);
        let at = rows
            .iter()
            .rposition(|row| row.start <= column)
            .unwrap_or(0);
        let start = rows.get(at).map_or(0, |row| row.start);
        (self.top_of(line) + at, column.saturating_sub(start))
    }
}

/// How many columns a character takes. ASCII is the common case and always
/// one, apart from control characters, which are drawn as nothing.
fn char_width(c: char) -> usize {
    if c.is_ascii() {
        return usize::from(!c.is_ascii_control());
    }
    Line::raw(c.to_string()).width()
}

/// `line` broken into rows of at most `width` columns, at spaces where it
/// can be, and in the middle of a word too long to fit.
fn wrap(line: &str, width: usize) -> Vec<Row> {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return vec![Row {
            start: 0,
            text: String::new(),
        }];
    }
    let mut rows = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        // How many characters fit from `start`.
        let mut used = 0;
        let mut end = start;
        while end < chars.len() {
            let w = char_width(chars[end]);
            if used + w > width && end > start {
                break;
            }
            used += w;
            end += 1;
        }
        if end >= chars.len() {
            rows.push(row(&chars, start, chars.len()));
            break;
        }
        // The character that did not fit ends the row when it is a space.
        // Otherwise the row backs up to the last space that does fit, and
        // failing that breaks the word.
        let at = if chars[end] == ' ' {
            end
        } else {
            (start + 1..end)
                .rev()
                .find(|&i| chars[i] == ' ')
                .unwrap_or(end)
        };
        rows.push(row(&chars, start, at));
        // The next row starts past the spaces the break left behind.
        start = at;
        while start < chars.len() && chars[start] == ' ' {
            start += 1;
        }
        if start >= chars.len() {
            break;
        }
    }
    rows
}

/// The row of `chars` from `start` up to `end`, without the spaces it ends
/// with, which the break left behind.
fn row(chars: &[char], start: usize, end: usize) -> Row {
    let text: String = chars[start..end].iter().collect();
    Row {
        start,
        text: text.trim_end().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(line: &str, width: usize) -> Vec<String> {
        wrap(line, width).into_iter().map(|row| row.text).collect()
    }

    #[test]
    fn a_short_line_is_one_row() {
        assert_eq!(rows("Dear Charles,", 40), ["Dear Charles,"]);
    }

    #[test]
    fn a_line_that_just_fits_is_one_row() {
        assert_eq!(rows("abcde", 5), ["abcde"]);
    }

    #[test]
    fn an_empty_line_is_one_empty_row() {
        assert_eq!(rows("", 10), [""]);
    }

    #[test]
    fn a_long_line_breaks_at_a_space_and_the_space_goes() {
        assert_eq!(rows("aaa bbb ccc", 7), ["aaa bbb", "ccc"]);
        assert_eq!(rows("the engine weaves", 10), ["the engine", "weaves"]);
    }

    #[test]
    fn a_word_too_long_for_a_row_is_broken() {
        assert_eq!(rows("abcdefghij", 4), ["abcd", "efgh", "ij"]);
    }

    #[test]
    fn indentation_is_kept_on_the_first_row() {
        assert_eq!(rows("    indented words", 12), ["    indented", "words"]);
    }

    #[test]
    fn a_row_never_starts_with_the_space_it_broke_at() {
        for row in rows("aaa    bbb", 5) {
            assert!(!row.starts_with(' '), "{row:?}");
        }
    }

    #[test]
    fn wrapping_narrows_to_one_column_without_looping_forever() {
        assert_eq!(rows("ab", 1), ["a", "b"]);
    }

    #[test]
    fn every_character_is_kept_once_the_spaces_broken_at_are_dropped() {
        let line = "The engine weaves algebraic patterns as the loom weaves flowers";
        let joined: String = rows(line, 9)
            .concat()
            .chars()
            .filter(|c| *c != ' ')
            .collect();
        let original: String = line.chars().filter(|c| *c != ' ').collect();

        assert_eq!(joined, original);
    }

    fn wrapped(lines: &[&str], width: usize) -> Wrapped {
        let lines: Vec<String> = lines.iter().map(|line| (*line).to_owned()).collect();
        Wrapped::new(&lines, width)
    }

    #[test]
    fn rows_are_counted_and_found_per_line() {
        let w = wrapped(&["aaa bbb ccc", "", "ddd"], 7);

        assert_eq!(w.height(0), 2);
        assert_eq!(w.height(1), 1);
        assert_eq!(w.top_of(0), 0);
        assert_eq!(w.top_of(1), 2);
        assert_eq!(w.top_of(2), 3);
        assert_eq!(w.total(), 4);
    }

    #[test]
    fn a_place_is_found_on_the_row_it_is_wrapped_to() {
        let w = wrapped(&["aaa bbb ccc"], 7);

        assert_eq!(w.place(0, 0), (0, 0));
        assert_eq!(w.place(0, 6), (0, 6), "the last of the first row");
        assert_eq!(w.place(0, 8), (1, 0), "the row after the space");
        assert_eq!(w.place(0, 10), (1, 2));
        assert_eq!(w.place(0, 11), (1, 3), "past the end of the line");
    }

    #[test]
    fn a_place_on_a_later_line_counts_the_rows_above_it() {
        let w = wrapped(&["aaa bbb ccc", "ddd"], 7);

        assert_eq!(w.place(1, 0), (2, 0));
    }
}
