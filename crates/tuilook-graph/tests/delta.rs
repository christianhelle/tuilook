mod support;

use jiff::{SignedDuration, Timestamp};
use serde_json::{Value, json};
use tuilook_graph::GraphBackend;
use tuilook_model::{
    BackendError, DeltaPage, FolderId, Importance, MailBackend, Message, MessageId,
};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn backend(server: &MockServer) -> GraphBackend<std::sync::Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
}

fn graph_message(id: &str, subject: &str) -> Value {
    json!({
        "id": id,
        "parentFolderId": "inbox",
        "conversationId": "conv-1",
        "subject": subject,
        "from": { "emailAddress": { "name": "Ada Lovelace", "address": "ada@example.com" } },
        "receivedDateTime": "2026-10-24T08:15:00Z",
        "isRead": true,
        "flag": { "flagStatus": "flagged" },
        "importance": "high",
        "hasAttachments": true,
        "bodyPreview": "Attached are my notes"
    })
}

fn message(id: &str, subject: &str) -> Message {
    Message {
        id: MessageId(id.to_owned()),
        folder_id: FolderId("inbox".to_owned()),
        conversation_id: "conv-1".to_owned(),
        subject: subject.to_owned(),
        from_name: "Ada Lovelace".to_owned(),
        from_addr: "ada@example.com".to_owned(),
        received_at: "2026-10-24T08:15:00Z".parse().unwrap(),
        is_read: true,
        flagged: true,
        importance: Importance::High,
        has_attachments: true,
        preview: "Attached are my notes".to_owned(),
    }
}

/// Matches a `$filter` of `receivedDateTime ge <about 90 days ago>`.
fn filters_last_90_days(request: &Request) -> bool {
    let Some((_, filter)) = request.url.query_pairs().find(|(k, _)| k == "$filter") else {
        return false;
    };
    let Some(since) = filter.strip_prefix("receivedDateTime ge ") else {
        return false;
    };
    let Ok(since) = since.parse::<Timestamp>() else {
        return false;
    };
    let expected = Timestamp::now() - SignedDuration::from_hours(90 * 24);
    since.duration_since(expected).abs() < SignedDuration::from_mins(5)
}

#[tokio::test]
async fn first_sync_pages_through_the_last_90_days_to_a_delta_link() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/mailFolders/inbox/messages/delta"))
        .and(query_param(
            "$select",
            "id,conversationId,subject,from,receivedDateTime,isRead,flag,importance,hasAttachments,bodyPreview",
        ))
        .and(filters_last_90_days)
        .and(header("prefer", "odata.maxpagesize=50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [graph_message("m1", "First")],
            "@odata.nextLink": format!("{}/delta/page2", server.uri())
        })))
        .mount(&server)
        .await;
    Mock::given(path("/delta/page2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [graph_message("m2", "Second")],
            "@odata.deltaLink": "https://graph.example/delta?token=abc"
        })))
        .mount(&server)
        .await;

    let page = backend(&server)
        .delta_sync(&FolderId("inbox".to_owned()), None)
        .await
        .unwrap();

    assert_eq!(
        page,
        DeltaPage {
            upserted: vec![message("m1", "First"), message("m2", "Second")],
            removed: vec![],
            delta_link: Some("https://graph.example/delta?token=abc".to_owned()),
        }
    );
}

#[tokio::test]
async fn later_syncs_follow_the_delta_link_and_report_removals() {
    let server = MockServer::start().await;
    let delta_link = format!("{}/delta?token=abc", server.uri());
    Mock::given(path("/delta"))
        .and(query_param("token", "abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                graph_message("m1", "Edited"),
                { "id": "m2", "@removed": { "reason": "deleted" } }
            ],
            "@odata.deltaLink": "https://graph.example/delta?token=def"
        })))
        .mount(&server)
        .await;

    let page = backend(&server)
        .delta_sync(&FolderId("inbox".to_owned()), Some(&delta_link))
        .await
        .unwrap();

    assert_eq!(page.upserted, [message("m1", "Edited")]);
    assert_eq!(page.removed, [MessageId("m2".to_owned())]);
    assert_eq!(
        page.delta_link.as_deref(),
        Some("https://graph.example/delta?token=def")
    );
}

#[tokio::test]
async fn an_expired_delta_link_asks_for_a_full_resync() {
    let server = MockServer::start().await;
    Mock::given(path("/delta"))
        .respond_with(ResponseTemplate::new(410).set_body_json(json!({
            "error": { "code": "SyncStateNotFound", "message": "The sync state is expired." }
        })))
        .mount(&server)
        .await;

    let error = backend(&server)
        .delta_sync(
            &FolderId("inbox".to_owned()),
            Some(&format!("{}/delta?token=old", server.uri())),
        )
        .await
        .unwrap_err();

    assert_eq!(error, BackendError::DeltaExpired);
}
