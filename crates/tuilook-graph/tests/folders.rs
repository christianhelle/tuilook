mod support;

use serde_json::json;
use tuilook_graph::GraphBackend;
use tuilook_model::{Folder, FolderId, MailBackend, WellKnown};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
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
