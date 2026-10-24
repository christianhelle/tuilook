mod support;

use serde_json::json;
use tuilook_graph::GraphBackend;
use tuilook_model::{Body, BodyType, MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

#[tokio::test]
async fn fetches_the_html_body_of_a_message() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/messages/m1"))
        .and(query_param("$select", "body"))
        .and(header("prefer", r#"outlook.body-content-type="html""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m1",
            "body": { "contentType": "html", "content": "<p>Hello</p>" }
        })))
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    let body = backend
        .fetch_body(&MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert_eq!(
        body,
        Body {
            content_type: BodyType::Html,
            content: "<p>Hello</p>".to_owned()
        }
    );
}
