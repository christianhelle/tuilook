use std::collections::{HashMap, HashSet};

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
    pub const ALL: [Self; 6] = [
        Self::Inbox,
        Self::Drafts,
        Self::SentItems,
        Self::Archive,
        Self::DeletedItems,
        Self::JunkEmail,
    ];

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

/// Orders folders the way the folder tree shows them: each folder is
/// followed by its subfolders. Siblings put well-known folders first in
/// Outlook order, then the rest alphabetically. A folder whose parent is
/// not in the list is shown at the top level.
pub fn sort_folders(folders: &mut [Folder]) {
    folders.sort_by_cached_key(|f| (f.well_known.is_none(), f.well_known, f.name.to_lowercase()));

    let ids: HashSet<&FolderId> = folders.iter().map(|f| &f.id).collect();
    let mut children: HashMap<Option<&FolderId>, Vec<usize>> = HashMap::new();
    for (i, f) in folders.iter().enumerate() {
        let parent = f.parent_id.as_ref().filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(i);
    }

    let mut order = Vec::with_capacity(folders.len());
    let mut visited = vec![false; folders.len()];
    // Depth-first; reversed pushes keep siblings in sorted order.
    let mut stack: Vec<usize> = children
        .get(&None)
        .into_iter()
        .flatten()
        .rev()
        .copied()
        .collect();
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut visited[i], true) {
            continue;
        }
        order.push(i);
        if let Some(kids) = children.get(&Some(&folders[i].id)) {
            stack.extend(kids.iter().rev());
        }
    }
    // Folders caught in a parent cycle are never reached from a root.
    order.extend((0..folders.len()).filter(|&i| !visited[i]));

    let sorted: Vec<Folder> = order.iter().map(|&i| folders[i].clone()).collect();
    folders.clone_from_slice(&sorted);
}

/// How deeply each folder is nested: 0 for top-level folders.
pub fn folder_depths(folders: &[Folder]) -> Vec<usize> {
    let parents: HashMap<&FolderId, Option<&FolderId>> = folders
        .iter()
        .map(|f| (&f.id, f.parent_id.as_ref()))
        .collect();
    folders
        .iter()
        .map(|f| {
            let mut seen = HashSet::from([&f.id]);
            let mut parent = f.parent_id.as_ref();
            let mut depth = 0;
            while let Some(p) = parent.filter(|p| parents.contains_key(p) && seen.insert(p)) {
                depth += 1;
                parent = parents[p];
            }
            depth
        })
        .collect()
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

    fn child(name: &str, parent: &str) -> Folder {
        Folder {
            parent_id: Some(FolderId(parent.to_owned())),
            ..folder(name, None)
        }
    }

    #[test]
    fn subfolders_follow_their_parent_depth_first() {
        let mut folders = vec![
            child("2026", "Receipts"),
            folder("Projects", None),
            child("Receipts", "Inbox"),
            child("Alpha", "Projects"),
            folder("Inbox", Some(WellKnown::Inbox)),
            child("Bills", "Inbox"),
        ];

        sort_folders(&mut folders);

        let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            ["Inbox", "Bills", "Receipts", "2026", "Projects", "Alpha"]
        );
    }

    #[test]
    fn folders_whose_parent_is_missing_are_shown_at_the_top_level() {
        let mut folders = vec![
            child("Orphan", "gone"),
            folder("Inbox", Some(WellKnown::Inbox)),
        ];

        sort_folders(&mut folders);

        let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Inbox", "Orphan"]);
    }

    #[test]
    fn depth_counts_the_parents_above_each_folder() {
        let folders = vec![
            folder("Inbox", None),
            child("Receipts", "Inbox"),
            child("2026", "Receipts"),
            child("Orphan", "gone"),
        ];

        assert_eq!(folder_depths(&folders), [0, 1, 2, 0]);
    }

    #[test]
    fn depth_stops_at_a_parent_cycle() {
        let folders = vec![child("A", "B"), child("B", "A")];

        assert_eq!(folder_depths(&folders), [1, 1]);
    }

    #[test]
    fn folders_in_a_parent_cycle_are_still_listed_once() {
        let mut folders = vec![child("A", "B"), child("B", "A"), folder("Inbox", None)];

        sort_folders(&mut folders);

        let names: Vec<_> = folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Inbox", "A", "B"]);
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
        for kind in WellKnown::ALL {
            assert_eq!(WellKnown::from_graph_name(kind.as_graph_name()), Some(kind));
        }
    }

    #[test]
    fn unknown_names_are_not_well_known() {
        assert_eq!(WellKnown::from_graph_name("projects"), None);
    }
}
