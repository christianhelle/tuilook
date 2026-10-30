mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use tuilook::model::{MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

fn backend(server: &MockServer) -> GraphBackend<std::sync::Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
}

async fn expect_flag_status(server: &MockServer, status: &str) {
    Mock::given(method("PATCH"))
        .and(path("/me/messages/m1"))
        .and(body_json(json!({ "flag": { "flagStatus": status } })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "m1" })))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn flagging_sets_the_flag_status_to_flagged() {
    let server = MockServer::start().await;
    expect_flag_status(&server, "flagged").await;

    backend(&server)
        .set_flagged(&MessageId("m1".to_owned()), true)
        .await
        .unwrap();
}

#[tokio::test]
async fn unflagging_sets_the_flag_status_to_not_flagged() {
    let server = MockServer::start().await;
    expect_flag_status(&server, "notFlagged").await;

    backend(&server)
        .set_flagged(&MessageId("m1".to_owned()), false)
        .await
        .unwrap();
}
