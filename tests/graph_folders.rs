mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use tuilook::model::{Folder, FolderId, MailBackend, WellKnown};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param},
};

fn backend(server: &MockServer) -> GraphBackend<std::sync::Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
}

fn graph_folder(id: &str, name: &str, parent: &str, children: u32) -> serde_json::Value {
    json!({
        "id": id,
        "displayName": name,
        "parentFolderId": parent,
        "childFolderCount": children,
        "unreadItemCount": 2,
        "totalItemCount": 9
    })
}

fn folder(id: &str, name: &str, parent: Option<&str>, well_known: Option<WellKnown>) -> Folder {
    Folder {
        id: FolderId(id.to_owned()),
        parent_id: parent.map(|p| FolderId(p.to_owned())),
        name: name.to_owned(),
        well_known,
        unread: 2,
        total: 9,
    }
}

fn page(value: Vec<serde_json::Value>, next: Option<String>) -> ResponseTemplate {
    let mut body = json!({ "value": value });
    if let Some(next) = next {
        body["@odata.nextLink"] = json!(next);
    }
    ResponseTemplate::new(200).set_body_json(body)
}

#[tokio::test]
async fn lists_top_level_folders_across_pages() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/mailFolders"))
        .and(query_param("$top", "100"))
        .respond_with(page(
            vec![graph_folder("f1", "Inbox", "root", 0)],
            Some(format!("{}/me/mailFolders/page2", server.uri())),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me/mailFolders/page2"))
        .respond_with(page(vec![graph_folder("f2", "Projects", "root", 0)], None))
        .mount(&server)
        .await;

    let folders = backend(&server).list_folders().await.unwrap();

    assert_eq!(
        folders,
        [
            folder("f1", "Inbox", None, None),
            folder("f2", "Projects", None, None)
        ]
    );
}

#[tokio::test]
async fn child_folders_are_fetched_recursively() {
    let server = MockServer::start().await;
    Mock::given(path("/me/mailFolders"))
        .respond_with(page(vec![graph_folder("f1", "Inbox", "root", 1)], None))
        .mount(&server)
        .await;
    Mock::given(path("/me/mailFolders/f1/childFolders"))
        .and(query_param("$top", "100"))
        .respond_with(page(vec![graph_folder("f2", "Receipts", "f1", 1)], None))
        .mount(&server)
        .await;
    Mock::given(path("/me/mailFolders/f2/childFolders"))
        .respond_with(page(vec![graph_folder("f3", "2026", "f2", 0)], None))
        .mount(&server)
        .await;

    let folders = backend(&server).list_folders().await.unwrap();

    assert_eq!(
        folders,
        [
            folder("f1", "Inbox", None, None),
            folder("f2", "Receipts", Some("f1"), None),
            folder("f3", "2026", Some("f2"), None),
        ]
    );
}

#[tokio::test]
async fn well_known_folders_are_identified_and_missing_ones_skipped() {
    let server = MockServer::start().await;
    Mock::given(path("/me/mailFolders"))
        .respond_with(page(
            vec![
                graph_folder("f1", "Inbox", "root", 0),
                graph_folder("f2", "Sent Items", "root", 0),
                graph_folder("f3", "Projects", "root", 0),
            ],
            None,
        ))
        .mount(&server)
        .await;
    for (alias, id) in [("inbox", "f1"), ("sentitems", "f2")] {
        Mock::given(path(format!("/me/mailFolders/{alias}")))
            .and(query_param("$select", "id"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": id })))
            .mount(&server)
            .await;
    }
    // Archive and the rest are left unmocked, so Graph answers 404.

    let folders = backend(&server).list_folders().await.unwrap();

    assert_eq!(
        folders,
        [
            folder("f1", "Inbox", None, Some(WellKnown::Inbox)),
            folder("f2", "Sent Items", None, Some(WellKnown::SentItems)),
            folder("f3", "Projects", None, None),
        ]
    );
}

#[tokio::test]
async fn well_known_folders_are_looked_up_only_on_the_first_sync() {
    let server = MockServer::start().await;
    Mock::given(path("/me/mailFolders"))
        .respond_with(page(vec![graph_folder("f1", "Inbox", "root", 0)], None))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(path("/me/mailFolders/inbox"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "f1" })))
        .expect(1)
        .mount(&server)
        .await;
    let backend = backend(&server);

    backend.list_folders().await.unwrap();
    let folders = backend.list_folders().await.unwrap();

    assert_eq!(folders[0].well_known, Some(WellKnown::Inbox));
}

#[tokio::test]
async fn creating_a_top_level_folder_posts_its_name() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/mailFolders"))
        .and(body_json(json!({"displayName": "Receipts"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "f9"})))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .create_folder(None, "Receipts")
        .await
        .unwrap();
}

#[tokio::test]
async fn creating_a_subfolder_posts_to_its_parent() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/mailFolders/inbox-id/childFolders"))
        .and(body_json(json!({"displayName": "2026"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "f9"})))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .create_folder(Some(&FolderId("inbox-id".to_owned())), "2026")
        .await
        .unwrap();
}

#[tokio::test]
async fn renaming_a_folder_patches_its_name() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/me/mailFolders/f1"))
        .and(body_json(json!({"displayName": "Bills"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "f1"})))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .rename_folder(&FolderId("f1".to_owned()), "Bills")
        .await
        .unwrap();
}
