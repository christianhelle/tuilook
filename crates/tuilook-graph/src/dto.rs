//! Graph JSON shapes. Only the fields tuilook selects.

use serde::Deserialize;

/// One page of a Graph collection.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MailFolder {
    pub id: String,
    pub display_name: String,
    pub parent_folder_id: Option<String>,
    #[serde(default)]
    pub child_folder_count: u32,
    #[serde(default)]
    pub unread_item_count: u32,
    #[serde(default)]
    pub total_item_count: u32,
}

#[derive(Debug, Deserialize)]
pub struct Id {
    pub id: String,
}
