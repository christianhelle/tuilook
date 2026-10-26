mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

async fn signed_in_as(user: serde_json::Value) -> Option<String> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(query_param("$select", "mail,userPrincipalName"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user))
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend.address().await.unwrap()
}

#[tokio::test]
async fn the_address_is_the_mailbox_address() {
    let address = signed_in_as(json!({
        "mail": "me@work.example",
        "userPrincipalName": "me@tenant.onmicrosoft.com"
    }))
    .await;

    assert_eq!(address.as_deref(), Some("me@work.example"));
}

#[tokio::test]
async fn without_a_mailbox_address_the_sign_in_name_stands_in() {
    let address = signed_in_as(json!({
        "mail": null,
        "userPrincipalName": "me@outlook.com"
    }))
    .await;

    assert_eq!(address.as_deref(), Some("me@outlook.com"));
}
