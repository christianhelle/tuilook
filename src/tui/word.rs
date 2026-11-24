//! What counts as a word, for moving and deleting by words.

/// What kind of character, for moving by words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Space,
    Word,
    Punctuation,
}

/// The class of a character; `None` counts as a space.
pub fn class_of(c: Option<char>) -> Class {
    match c {
        None => Class::Space,
        Some(c) if c.is_whitespace() => Class::Space,
        Some(c) if c.is_alphanumeric() || c == '_' => Class::Word,
        Some(_) => Class::Punctuation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_digits_and_underscores_are_words() {
        for c in ['a', 'Z', '0', '_', 'é', 'ß'] {
            assert_eq!(class_of(Some(c)), Class::Word, "{c}");
        }
    }

    #[test]
    fn whitespace_is_space() {
        for c in [' ', '\t', '\u{a0}'] {
            assert_eq!(class_of(Some(c)), Class::Space, "{c:?}");
        }
    }

    #[test]
    fn anything_else_is_punctuation() {
        for c in ['-', '@', '.', '"', '(', '€'] {
            assert_eq!(class_of(Some(c)), Class::Punctuation, "{c}");
        }
    }

    #[test]
    fn no_character_past_the_end_counts_as_space() {
        assert_eq!(class_of(None), Class::Space);
    }
}
