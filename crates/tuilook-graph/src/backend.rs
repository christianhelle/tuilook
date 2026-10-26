use std::collections::{HashMap, HashSet};

use jiff::{SignedDuration, Timestamp, Unit};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::json;
use tuilook_auth::TokenSource;
use tuilook_model::{
    BackendError, Body, BodyType, ComposeKind, DeltaPage, Draft, Folder, FolderId, Importance,
    MailBackend, Message, MessageId, WellKnown,
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

    async fn delete(&self, message: &MessageId, permanent: bool) -> Result<(), BackendError> {
        if permanent {
            let path = format!("/me/messages/{}/permanentDelete", message.0);
            self.client.post_no_content(&path, None).await?;
        } else {
            let path = format!("/me/messages/{}/move", message.0);
            let target = json!({ "destinationId": WellKnown::DeletedItems.as_graph_name() });
            self.client.post::<dto::Id>(&path, &target).await?;
        }
        Ok(())
    }

    async fn send(&self, draft: &Draft) -> Result<(), BackendError> {
        let (original, create) = match &draft.kind {
            ComposeKind::New => {
                let message = json!({
                    "subject": draft.subject,
                    "body": { "contentType": "Text", "content": draft.body },
                    "toRecipients": recipients(&draft.to),
                    "ccRecipients": recipients(&draft.cc),
                });
                let mail = json!({ "message": message, "saveToSentItems": true });
                self.client
                    .post_no_content("/me/sendMail", Some(&mail))
                    .await?;
                return Ok(());
            }
            ComposeKind::Reply(id) => (id, "createReply"),
            ComposeKind::ReplyAll(id) => (id, "createReplyAll"),
            ComposeKind::Forward(id) => (id, "createForward"),
        };
        // Graph drafts the reply so the original is quoted and threaded the
        // way Outlook does it; tuilook puts the user's text above the quote.
        let path = format!("/me/messages/{}/{create}", original.0);
        let created: dto::DraftMessage = self.client.post(&path, &json!({})).await?;
        let (to, cc) = match draft.kind {
            ComposeKind::ReplyAll(_) => (
                merge(&created.to_recipients, &draft.to),
                merge(&created.cc_recipients, &draft.cc),
            ),
            _ => (draft.to.clone(), draft.cc.clone()),
        };
        let (content_type, content) = above_quote(&draft.body, &created.body);
        let changes = json!({
            "subject": draft.subject,
            "body": { "contentType": content_type, "content": content },
            "toRecipients": recipients(&to),
            "ccRecipients": recipients(&cc),
        });
        let path = format!("/me/messages/{}", created.id);
        self.client.patch(&path, &changes).await?;
        self.client
            .post_no_content(&format!("{path}/send"), None)
            .await?;
        Ok(())
    }
}

fn recipients(addresses: &[String]) -> Vec<serde_json::Value> {
    addresses
        .iter()
        .map(|address| json!({ "emailAddress": { "address": address } }))
        .collect()
}

/// The addresses Graph picked, then any the user added.
fn merge(picked: &[dto::Recipient], added: &[String]) -> Vec<String> {
    let mut addresses: Vec<String> = picked
        .iter()
        .map(|r| r.email_address.address.clone())
        .collect();
    for address in added {
        if !addresses.iter().any(|a| a.eq_ignore_ascii_case(address)) {
            addresses.push(address.clone());
        }
    }
    addresses
}

/// Puts plain `text` above the quoted original in a drafted body.
fn above_quote(text: &str, quoted: &dto::ItemBody) -> (&'static str, String) {
    if !quoted.content_type.eq_ignore_ascii_case("html") {
        return ("Text", format!("{text}\n\n{}", quoted.content));
    }
    let html = format!("<div>{}</div>", escape_html(text).replace('\n', "<br>"));
    let content = &quoted.content;
    // Just inside <body ...>, or at the very start when there is none.
    let at = content
        .find("<body")
        .and_then(|start| content[start..].find('>').map(|end| start + end + 1))
        .unwrap_or(0);
    (
        "HTML",
        format!("{}{html}{}", &content[..at], &content[at..]),
    )
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn to_message(m: dto::Message, folder: &FolderId) -> Message {
    let (from_name, from_addr) = m
        .from
        .map(|r| (r.email_address.name, r.email_address.address))
        .unwrap_or_default();
    Message {
        id: MessageId(m.id),
        folder_id: folder.clone(),
        conversation_id: m.conversation_id.unwrap_or_default(),
        subject: m.subject.unwrap_or_default(),
        from_name,
        from_addr,
        received_at: m.received_date_time.unwrap_or(Timestamp::UNIX_EPOCH),
        // Without a read state, do not nag the user with a bold row.
        is_read: m.is_read.unwrap_or(true),
        flagged: m.flag.is_some_and(|f| f.flag_status == "flagged"),
        importance: match m.importance.as_deref() {
            Some("low") => Importance::Low,
            Some("high") => Importance::High,
            _ => Importance::Normal,
        },
        has_attachments: m.has_attachments.unwrap_or(false),
        preview: m.body_preview.unwrap_or_default(),
    }
}

impl From<GraphError> for BackendError {
    fn from(e: GraphError) -> Self {
        BackendError::Other(e.to_string())
    }
}
