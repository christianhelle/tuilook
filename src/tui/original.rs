//! The message being replied to or forwarded, shown beside the message
//! being written.

use crate::tui::viewer::Viewer;

/// The pane holding the original: whether it is shown, and the cursor that
/// scrolls it and picks text to copy.
#[derive(Debug)]
pub struct Original {
    /// Whether the pane is shown beside the body.
    pub shown: bool,
    /// The cursor, which scrolls and picks text to copy.
    pub viewer: Viewer,
}

impl Default for Original {
    fn default() -> Self {
        Original {
            shown: true,
            viewer: Viewer::default(),
        }
    }
}

impl Original {
    /// Hides the pane, or shows it again.
    pub fn toggle(&mut self) {
        self.shown = !self.shown;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_original_starts_shown_and_toggles() {
        let mut original = Original::default();
        assert!(original.shown);

        original.toggle();
        assert!(!original.shown);

        original.toggle();
        assert!(original.shown);
    }
}
