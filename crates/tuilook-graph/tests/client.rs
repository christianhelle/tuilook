mod support;

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn get_sends_the_bearer_token_and_decodes_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(header("authorization", "Bearer token-1"))
        .and(header("prefer", "odata.maxpagesize=50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "me"})))
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);

    let me: Value = client
        .get("/me", &[("Prefer", "odata.maxpagesize=50")])
        .await
        .unwrap();

    assert_eq!(me, json!({"id": "me"}));
}
