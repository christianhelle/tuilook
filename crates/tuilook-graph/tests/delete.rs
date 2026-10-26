mod support;

use serde_json::json;
use tuilook_graph::GraphBackend;
use tuilook_model::{MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

#[tokio::test]
async fn deleting_moves_the_message_to_deleted_items() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/messages/m1/move"))
        .and(body_json(json!({"destinationId": "deleteditems"})))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": "m1-moved"})))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend
        .delete(&MessageId("m1".to_owned()), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn deleting_permanently_skips_deleted_items() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/messages/m1/permanentDelete"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend
        .delete(&MessageId("m1".to_owned()), true)
        .await
        .unwrap();
}
