//! Graph JSON shapes. Only the fields tuilook selects.

use serde::Deserialize;

/// One page of a Graph collection.
#[derive(Debug, Deserialize)]
pub struct Page<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
    #[serde(rename = "@odata.deltaLink")]
    pub delta_link: Option<String>,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    #[serde(rename = "@removed")]
    pub removed: Option<serde::de::IgnoredAny>,
    // Graph may send null for any of these, so none is required.
    pub conversation_id: Option<String>,
    pub subject: Option<String>,
    pub from: Option<Recipient>,
    pub received_date_time: Option<jiff::Timestamp>,
    pub is_read: Option<bool>,
    pub flag: Option<Flag>,
    pub importance: Option<String>,
    pub has_attachments: Option<bool>,
    pub body_preview: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recipient {
    pub email_address: EmailAddress,
}

#[derive(Debug, Deserialize)]
pub struct EmailAddress {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub address: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Flag {
    pub flag_status: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageBody {
    pub body: ItemBody,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemBody {
    pub content_type: String,
    #[serde(default)]
    pub content: String,
}
