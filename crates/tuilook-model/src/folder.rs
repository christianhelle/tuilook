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

    pub fn as_graph_name(self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Drafts => "drafts",
            Self::SentItems => "sentitems",
            Self::Archive => "archive",
            Self::DeletedItems => "deleteditems",
            Self::JunkEmail => "junkemail",
        }
    }
}

/// Graph folder id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FolderId(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub parent_id: Option<FolderId>,
    pub name: String,
    pub well_known: Option<WellKnown>,
    pub unread: u32,
    pub total: u32,
}

/// Orders folders the way the folder tree shows them: well-known folders
/// first in Outlook order, then the rest alphabetically.
pub fn sort_folders(folders: &mut [Folder]) {
    folders.sort_by_cached_key(|f| (f.well_known.is_none(), f.well_known, f.name.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str, well_known: Option<WellKnown>) -> Folder {
        Folder {
            id: FolderId(name.to_owned()),
            parent_id: None,
            name: name.to_owned(),
            well_known,
            unread: 0,
            total: 0,
        }
    }

    #[test]
    fn well_known_folders_sort_first_then_alphabetically() {
        let mut folders = vec![
            folder("projects", None),
            folder("Sent Items", Some(WellKnown::SentItems)),
            folder("Archive", None),
            folder("Inbox", Some(WellKnown::Inbox)),
            folder("bills", None),
        ];

        sort_folders(&mut folders);

        let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["Inbox", "Sent Items", "Archive", "bills", "projects"]
        );
    }

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
    fn graph_names_round_trip() {
        for kind in [
            WellKnown::Inbox,
            WellKnown::Drafts,
            WellKnown::SentItems,
            WellKnown::Archive,
            WellKnown::DeletedItems,
            WellKnown::JunkEmail,
        ] {
            assert_eq!(WellKnown::from_graph_name(kind.as_graph_name()), Some(kind));
        }
    }

    #[test]
    fn unknown_names_are_not_well_known() {
        assert_eq!(WellKnown::from_graph_name("projects"), None);
    }
}
