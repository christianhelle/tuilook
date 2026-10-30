use std::collections::{HashMap, HashSet};

use crate::auth::TokenSource;
use crate::model::{
    BackendError, Body, BodyType, ComposeKind, DeltaPage, Draft, Folder, FolderId, Importance,
    MailBackend, Message, MessageChange, MessageId, OlderPage, WellKnown,
};
use jiff::{SignedDuration, Timestamp, Unit};
use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::OnceCell;

use crate::graph::{
    GraphClient, GraphError,
    dto::{self, Page},
};

const MESSAGE_SELECT: &str = "id,conversationId,subject,from,receivedDateTime,isRead,flag,importance,hasAttachments,bodyPreview";

/// Outlook has no pinned field: it pins mail by moving both its renew
/// times to this date, far ahead of any real one.
const PINNED_AT: &str = "4500-09-01T00:00:00Z";

/// The renew times, PidTagRenewTime2 and PidTagRenewTime.
const RENEW_TIMES: [&str; 2] = ["SystemTime 0x0F01", "SystemTime 0x0F02"];

const PAGE_SIZE: (&str, &str) = ("Prefer", "odata.maxpagesize=50");

/// Messages fetched at a time from past a folder's sync window.
const OLDER_PAGE: usize = 50;

const FOLDER_SELECT: &str =
    "$select=id,displayName,parentFolderId,childFolderCount,unreadItemCount,totalItemCount";

/// [`MailBackend`] over Microsoft Graph.
pub struct GraphBackend<T> {
    client: GraphClient<T>,
    sync_window: SignedDuration,
    /// Well-known folder ids, looked up on the first sync. A folder keeps
    /// its id, so there is no need to ask again every sync.
    well_known: OnceCell<HashMap<String, WellKnown>>,
}

impl<T: TokenSource> GraphBackend<T> {
    pub fn new(client: GraphClient<T>) -> Self {
        Self {
            client,
            sync_window: SignedDuration::from_hours(90 * 24),
            well_known: OnceCell::new(),
        }
    }

    /// How far back the first sync of a folder reaches.
    pub fn with_sync_window(mut self, window: SignedDuration) -> Self {
        self.sync_window = window;
        self
    }

    /// The signed-in mailbox's email address. Personal accounts may have no
    /// `mail`, so the sign-in name stands in for it.
    pub async fn address(&self) -> Result<Option<String>, GraphError> {
        let user: dto::User = self
            .client
            .get("/me?$select=mail,userPrincipalName", &[])
            .await?;
        Ok(user
            .mail
            .filter(|address| !address.is_empty())
            .or(user.user_principal_name)
            .filter(|address| !address.is_empty()))
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
        let well_known = self
            .well_known
            .get_or_try_init(|| self.well_known_ids())
            .await?;
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
                } else if m.received_date_time.is_none() {
                    // Every message has a date, so this is only what changed.
                    result.changed.push(to_change(m));
                } else {
                    result.upserted.push(to_message(m, folder));
                }
            }
            next = page.next_link;
            result.delta_link = page.delta_link;
        }
        Ok(result)
    }

    async fn older_messages(
        &self,
        folder: &FolderId,
        before: Timestamp,
        unread_only: bool,
    ) -> Result<OlderPage, BackendError> {
        let before = before
            .round(Unit::Second)
            .map_err(|e| BackendError::Other(e.to_string()))?;
        // Graph wants the property sorted on to come first in the filter.
        let unread = if unread_only {
            " and isRead eq false"
        } else {
            ""
        };
        let path = format!(
            "/me/mailFolders/{}/messages?$select={MESSAGE_SELECT}&$filter=receivedDateTime le {before}{unread}&$orderby=receivedDateTime desc&$top={OLDER_PAGE}",
            folder.0
        );
        let page: Page<dto::Message> = self.client.get(&path, &[]).await?;
        Ok(OlderPage {
            more: page.next_link.is_some(),
            messages: page
                .value
                .into_iter()
                .map(|m| to_message(m, folder))
                .collect(),
        })
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
            let deleted = FolderId(WellKnown::DeletedItems.as_graph_name().to_owned());
            self.move_message(message, &deleted).await?;
        }
        Ok(())
    }

    async fn move_message(&self, message: &MessageId, to: &FolderId) -> Result<(), BackendError> {
        let path = format!("/me/messages/{}/move", message.0);
        let target = json!({ "destinationId": to.0 });
        self.client.post::<dto::Id>(&path, &target).await?;
        Ok(())
    }

    async fn delete_folder(&self, folder: &FolderId, permanent: bool) -> Result<(), BackendError> {
        if permanent {
            let path = format!("/me/mailFolders/{}/permanentDelete", folder.0);
            self.client.post_no_content(&path, None).await?;
        } else {
            let path = format!("/me/mailFolders/{}/move", folder.0);
            let target = json!({ "destinationId": WellKnown::DeletedItems.as_graph_name() });
            self.client.post::<dto::Id>(&path, &target).await?;
        }
        Ok(())
    }

    async fn create_folder(
        &self,
        parent: Option<&FolderId>,
        name: &str,
    ) -> Result<(), BackendError> {
        let path = match parent {
            Some(parent) => format!("/me/mailFolders/{}/childFolders", parent.0),
            None => "/me/mailFolders".to_owned(),
        };
        let folder = json!({ "displayName": name });
        self.client.post::<dto::Id>(&path, &folder).await?;
        Ok(())
    }

    async fn rename_folder(&self, folder: &FolderId, name: &str) -> Result<(), BackendError> {
        let path = format!("/me/mailFolders/{}", folder.0);
        self.client
            .patch(&path, &json!({ "displayName": name }))
            .await?;
        Ok(())
    }

    async fn mark_read(&self, message: &MessageId) -> Result<(), BackendError> {
        let path = format!("/me/messages/{}", message.0);
        self.client.patch(&path, &json!({ "isRead": true })).await?;
        Ok(())
    }

    async fn pinned_messages(&self, folder: &FolderId) -> Result<Vec<Message>, BackendError> {
        let path = format!(
            "/me/mailFolders/{}/messages?$select={MESSAGE_SELECT}&$filter=singleValueExtendedProperties/any(ep: ep/id eq '{}' and ep/value eq '{PINNED_AT}')",
            folder.0, RENEW_TIMES[1]
        );
        let messages: Vec<dto::Message> = self.get_all(&path).await?;
        Ok(messages
            .into_iter()
            .map(|m| Message {
                pinned: true,
                ..to_message(m, folder)
            })
            .collect())
    }

    /// Unpinning renews the message at when it came, so it sorts among the
    /// rest of the mail again.
    async fn set_pinned(&self, message: &MessageId, pinned: bool) -> Result<(), BackendError> {
        let path = format!("/me/messages/{}", message.0);
        let at = if pinned {
            PINNED_AT.to_owned()
        } else {
            let received: dto::Message = self
                .client
                .get(&format!("{path}?$select=receivedDateTime"), &[])
                .await?;
            received
                .received_date_time
                .unwrap_or(Timestamp::UNIX_EPOCH)
                .to_string()
        };
        let properties: Vec<_> = RENEW_TIMES
            .iter()
            .map(|id| json!({ "id": id, "value": at }))
            .collect();
        let changes = json!({ "singleValueExtendedProperties": properties });
        self.client.patch(&path, &changes).await?;
        Ok(())
    }

    async fn send(&self, draft: &Draft) -> Result<(), BackendError> {
        self.send_draft(draft).await.map_err(sending_error)
    }
}

impl<T: TokenSource> GraphBackend<T> {
    async fn send_draft(&self, draft: &Draft) -> Result<(), GraphError> {
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
        pinned: false,
        importance: importance(m.importance.as_deref()),
        has_attachments: m.has_attachments.unwrap_or(false),
        preview: m.body_preview.unwrap_or_default(),
    }
}

fn to_change(m: dto::Message) -> MessageChange {
    MessageChange {
        id: MessageId(m.id),
        conversation_id: m.conversation_id,
        subject: m.subject,
        from: m
            .from
            .map(|r| (r.email_address.name, r.email_address.address)),
        is_read: m.is_read,
        flagged: m.flag.map(|f| f.flag_status == "flagged"),
        importance: m.importance.as_deref().map(|i| importance(Some(i))),
        has_attachments: m.has_attachments,
        preview: m.body_preview,
    }
}

fn importance(importance: Option<&str>) -> Importance {
    match importance {
        Some("low") => Importance::Low,
        Some("high") => Importance::High,
        _ => Importance::Normal,
    }
}

/// Graph error codes for an account the mail service will not send from.
/// Microsoft does not document them for Outlook.com, so these are the codes
/// Exchange uses for blocked submission and suspended or over-quota senders.
const BLOCKED_CODES: [&str; 5] = [
    "submissionblocked",
    "sendingblocked",
    "submissionquotaexceeded",
    "accountsuspend",
    "accountdisabled",
];

/// Tells a send the service refused because the account is blocked from
/// sending apart from any other failure.
fn sending_error(e: GraphError) -> BackendError {
    if let GraphError::Status {
        status,
        code,
        message,
    } = &e
    {
        let code_says = BLOCKED_CODES
            .iter()
            .any(|blocked| code.to_lowercase().contains(blocked));
        let forbidden_says = *status == StatusCode::FORBIDDEN && {
            let message = message.to_lowercase();
            ["block", "suspend", "spam"]
                .iter()
                .any(|word| message.contains(word))
        };
        if code_says || forbidden_says {
            return BackendError::SendingBlocked(format!("{code}: {message}"));
        }
    }
    e.into()
}

impl From<GraphError> for BackendError {
    fn from(e: GraphError) -> Self {
        BackendError::Other(e.to_string())
    }
}
