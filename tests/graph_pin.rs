mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use tuilook::model::{FolderId, MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param},
};

fn backend(server: &MockServer) -> GraphBackend<std::sync::Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
}

/// Outlook pins mail by moving both renew times far into the future.
fn renew_times(at: &str) -> serde_json::Value {
    json!({
        "singleValueExtendedProperties": [
            { "id": "SystemTime 0x0F01", "value": at },
            { "id": "SystemTime 0x0F02", "value": at },
        ]
    })
}

#[tokio::test]
async fn a_folders_pinned_mail_is_the_mail_renewed_in_the_year_4500() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/mailFolders/inbox/messages"))
        .and(query_param(
            "$filter",
            "singleValueExtendedProperties/any(ep: ep/id eq 'SystemTime 0x0F02' and ep/value eq '4500-09-01T00:00:00Z')",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [{
                "id": "m1",
                "subject": "Boarding pass",
                "receivedDateTime": "2025-03-01T08:00:00Z",
            }],
        })))
        .expect(1)
        .mount(&server)
        .await;

    let pinned = backend(&server)
        .pinned_messages(&FolderId("inbox".to_owned()))
        .await
        .unwrap();

    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].id, MessageId("m1".to_owned()));
    assert_eq!(pinned[0].folder_id, FolderId("inbox".to_owned()));
    assert!(pinned[0].pinned);
}

#[tokio::test]
async fn pinning_renews_the_message_in_the_year_4500() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/me/messages/m1"))
        .and(body_json(renew_times("4500-09-01T00:00:00Z")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "m1" })))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .set_pinned(&MessageId("m1".to_owned()), true)
        .await
        .unwrap();
}

#[tokio::test]
async fn unpinning_renews_the_message_back_at_when_it_came() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/messages/m1"))
        .and(query_param("$select", "receivedDateTime"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m1",
            "receivedDateTime": "2025-03-01T08:00:00Z",
        })))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/me/messages/m1"))
        .and(body_json(renew_times("2025-03-01T08:00:00Z")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "m1" })))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .set_pinned(&MessageId("m1".to_owned()), false)
        .await
        .unwrap();
}
