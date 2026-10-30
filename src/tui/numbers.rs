//! Vim's hybrid line numbers: the cursor's line by its number, the others
//! by how far they are from it, so a count such as 5j can be read off.

/// Columns the numbers of `lines` lines take, with a space after them.
/// As Vim's numberwidth, room for at least three digits.
pub fn gutter_width(lines: usize) -> u16 {
    let digits = lines.max(1).ilog10() + 1;
    u16::try_from(digits.max(3) + 1).unwrap_or(u16::MAX)
}

/// The number shown beside line `index` while the cursor is on line
/// `cursor`, padded to `width` columns.
pub fn label(index: usize, cursor: usize, width: u16) -> String {
    let digits = usize::from(width).saturating_sub(1);
    if index == cursor {
        format!("{:<digits$} ", index + 1)
    } else {
        format!("{:>digits$} ", index.abs_diff(cursor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gutter_has_room_for_three_digits_and_a_space() {
        assert_eq!(gutter_width(0), 4);
        assert_eq!(gutter_width(1), 4);
        assert_eq!(gutter_width(999), 4);
        assert_eq!(gutter_width(1000), 5);
    }

    #[test]
    fn the_cursor_line_shows_its_number_on_the_left() {
        assert_eq!(label(0, 0, 4), "1   ");
        assert_eq!(label(41, 41, 4), "42  ");
    }

    #[test]
    fn other_lines_show_how_far_they_are_from_the_cursor() {
        assert_eq!(label(2, 5, 4), "  3 ");
        assert_eq!(label(7, 5, 4), "  2 ");
        assert_eq!(label(1004, 4, 5), "1000 ");
    }
}
