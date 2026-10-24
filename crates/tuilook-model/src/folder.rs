/// Folders Graph exposes under a well-known name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WellKnown {
    Inbox,
    Drafts,
    SentItems,
    Archive,
    DeletedItems,
    JunkEmail,
}

impl WellKnown {
    pub fn from_graph_name(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "inbox" => Some(Self::Inbox),
            "drafts" => Some(Self::Drafts),
            "sentitems" => Some(Self::SentItems),
            "archive" => Some(Self::Archive),
            "deleteditems" => Some(Self::DeletedItems),
            "junkemail" => Some(Self::JunkEmail),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_graph_well_known_names() {
        assert_eq!(WellKnown::from_graph_name("inbox"), Some(WellKnown::Inbox));
        assert_eq!(
            WellKnown::from_graph_name("sentitems"),
            Some(WellKnown::SentItems)
        );
        assert_eq!(
            WellKnown::from_graph_name("deleteditems"),
            Some(WellKnown::DeletedItems)
        );
        assert_eq!(
            WellKnown::from_graph_name("junkemail"),
            Some(WellKnown::JunkEmail)
        );
    }

    #[test]
    fn graph_names_are_case_insensitive() {
        assert_eq!(WellKnown::from_graph_name("Inbox"), Some(WellKnown::Inbox));
    }

    #[test]
    fn unknown_names_are_not_well_known() {
        assert_eq!(WellKnown::from_graph_name("projects"), None);
    }
}
