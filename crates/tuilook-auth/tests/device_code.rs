use std::time::Duration;

use serde_json::json;
use tuilook_auth::{OAuthClient, OAuthConfig};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

fn client(server: &MockServer) -> OAuthClient {
    OAuthClient::new(OAuthConfig {
        authority: server.uri(),
        tenant: "common".to_owned(),
        client_id: "client-123".to_owned(),
        scopes: vec!["offline_access".to_owned(), "Mail.Read".to_owned()],
    })
}

#[tokio::test]
async fn start_returns_the_code_the_user_must_enter() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/devicecode"))
        .and(body_string_contains("client_id=client-123"))
        .and(body_string_contains("scope=offline_access+Mail.Read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-code",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://microsoft.com/devicelogin",
            "expires_in": 900,
            "interval": 5,
            "message": "To sign in, use a web browser..."
        })))
        .expect(1)
        .mount(&server)
        .await;

    let code = client(&server).start_device_code().await.unwrap();

    assert_eq!(code.user_code, "ABCD-EFGH");
    assert_eq!(code.verification_uri, "https://microsoft.com/devicelogin");
    assert_eq!(code.message, "To sign in, use a web browser...");
    assert_eq!(code.interval, Duration::from_secs(5));
}
