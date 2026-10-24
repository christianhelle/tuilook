use std::collections::{HashMap, HashSet};

use jiff::{SignedDuration, Timestamp, Unit};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use tuilook_auth::TokenSource;
use tuilook_model::{
    BackendError, Body, BodyType, DeltaPage, Folder, FolderId, Importance, MailBackend, Message,
    MessageId, WellKnown,
};

use crate::{
    GraphClient, GraphError,
    dto::{self, Page},
};

const MESSAGE_SELECT: &str = "id,conversationId,subject,from,receivedDateTime,isRead,flag,importance,hasAttachments,bodyPreview";

const PAGE_SIZE: (&str, &str) = ("Prefer", "odata.maxpagesize=50");

const FOLDER_SELECT: &str =
    "$select=id,displayName,parentFolderId,childFolderCount,unreadItemCount,totalItemCount";

/// [`MailBackend`] over Microsoft Graph.
pub struct GraphBackend<T> {
    client: GraphClient<T>,
    sync_window: SignedDuration,
}

impl<T: TokenSource> GraphBackend<T> {
    pub fn new(client: GraphClient<T>) -> Self {
        Self {
            client,
            sync_window: SignedDuration::from_hours(90 * 24),
        }
    }

    /// How far back the first sync of a folder reaches.
    pub fn with_sync_window(mut self, window: SignedDuration) -> Self {
        self.sync_window = window;
        self
    }

    /// Maps folder ids to the well-known role Graph gives them. Mailboxes
    /// without a folder (often Archive) answer 404, which is skipped.
    async fn well_known_ids(&self) -> Result<HashMap<String, WellKnown>, GraphError> {
        let mut ids = HashMap::new();
        for kind in WellKnown::ALL {
            let path = format!("/me/mailFolders/{}?$select=id", kind.as_graph_name());
            match self.client.get::<dto::Id>(&path, &[]).await {
                Ok(folder) => {
                    ids.insert(folder.id, kind);
                }
                Err(GraphError::Status { status, .. }) if status == StatusCode::NOT_FOUND => {}
                Err(e) => return Err(e),
            }
        }
        Ok(ids)
    }

    /// Follows `@odata.nextLink` until the collection is exhausted.
    async fn get_all<R: DeserializeOwned>(&self, path: &str) -> Result<Vec<R>, GraphError> {
        let mut items = Vec::new();
        let mut next = Some(path.to_owned());
        while let Some(url) = next {
            let page: Page<R> = self.client.get(&url, &[]).await?;
            items.extend(page.value);
            next = page.next_link;
        }
        Ok(items)
    }
}

impl<T: TokenSource> MailBackend for GraphBackend<T> {
    async fn list_folders(&self) -> Result<Vec<Folder>, BackendError> {
        let mut graph_folders: Vec<dto::MailFolder> = self
            .get_all(&format!("/me/mailFolders?$top=100&{FOLDER_SELECT}"))
            .await?;
        // Breadth-first: append each folder's children as it is reached.
        let mut i = 0;
        while i < graph_folders.len() {
            if graph_folders[i].child_folder_count > 0 {
                let path = format!(
                    "/me/mailFolders/{}/childFolders?$top=100&{FOLDER_SELECT}",
                    graph_folders[i].id
                );
                let children = self.get_all(&path).await?;
                graph_folders.extend(children);
            }
            i += 1;
        }
        let ids: HashSet<_> = graph_folders.iter().map(|f| f.id.clone()).collect();
        let well_known = self.well_known_ids().await?;
        Ok(graph_folders
            .into_iter()
            .map(|f| Folder {
                // Top-level folders hang off the hidden root; treat them as roots.
                parent_id: f.parent_folder_id.filter(|p| ids.contains(p)).map(FolderId),
                well_known: well_known.get(&f.id).copied(),
                id: FolderId(f.id),
                name: f.display_name,
                unread: f.unread_item_count,
                total: f.total_item_count,
            })
            .collect())
    }

    async fn delta_sync(
        &self,
        folder: &FolderId,
        delta_link: Option<&str>,
    ) -> Result<DeltaPage, BackendError> {
        let mut next = Some(match delta_link {
            Some(link) => link.to_owned(),
            None => {
                let since = (Timestamp::now() - self.sync_window)
                    .round(Unit::Second)
                    .map_err(|e| BackendError::Other(e.to_string()))?;
                format!(
                    "/me/mailFolders/{}/messages/delta?$select={MESSAGE_SELECT}&$filter=receivedDateTime ge {since}",
                    folder.0
                )
            }
        });
        let mut result = DeltaPage::default();
        while let Some(url) = next {
            let page: Page<dto::Message> = match self.client.get(&url, &[PAGE_SIZE]).await {
                Err(GraphError::Status { status, .. }) if status == StatusCode::GONE => {
                    return Err(BackendError::DeltaExpired);
                }
                result => result?,
            };
            for m in page.value {
                if m.removed.is_some() {
                    result.removed.push(MessageId(m.id));
                } else {
                    result.upserted.push(to_message(m, folder));
                }
            }
            next = page.next_link;
            result.delta_link = page.delta_link;
        }
        Ok(result)
    }

    async fn fetch_body(&self, message: &MessageId) -> Result<Body, BackendError> {
        let path = format!("/me/messages/{}?$select=body", message.0);
        let prefer = ("Prefer", r#"outlook.body-content-type="html""#);
        let response: dto::MessageBody = self.client.get(&path, &[prefer]).await?;
        Ok(Body {
            content_type: if response.body.content_type.eq_ignore_ascii_case("html") {
                BodyType::Html
            } else {
                BodyType::Text
            },
            content: response.body.content,
        })
    }
}

fn to_message(m: dto::Message, folder: &FolderId) -> Message {
    let (from_name, from_addr) = m
        .from
        .map(|r| (r.email_address.name, r.email_address.address))
        .unwrap_or_default();
    Message {
        id: MessageId(m.id),
        folder_id: folder.clone(),
        conversation_id: m.conversation_id,
        subject: m.subject.unwrap_or_default(),
        from_name,
        from_addr,
        received_at: m.received_date_time.unwrap_or(Timestamp::UNIX_EPOCH),
        is_read: m.is_read,
        flagged: m.flag.is_some_and(|f| f.flag_status == "flagged"),
        importance: match m.importance.as_deref() {
            Some("low") => Importance::Low,
            Some("high") => Importance::High,
            _ => Importance::Normal,
        },
        has_attachments: m.has_attachments,
        preview: m.body_preview,
    }
}

impl From<GraphError> for BackendError {
    fn from(e: GraphError) -> Self {
        BackendError::Other(e.to_string())
    }
}
