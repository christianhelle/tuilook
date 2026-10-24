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

async fn mount_device_code(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/devicecode"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "device_code": "dev-code",
            "user_code": "ABCD-EFGH",
            "verification_uri": "https://microsoft.com/devicelogin",
            "expires_in": 900,
            "interval": 5,
            "message": "To sign in..."
        })))
        .mount(server)
        .await;
}

fn oauth_error(error: &str) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(json!({
        "error": error,
        "error_description": format!("{error} description"),
    }))
}

fn token_endpoint() -> wiremock::MockBuilder {
    Mock::given(method("POST")).and(path("/common/oauth2/v2.0/token"))
}

#[tokio::test(start_paused = true)]
async fn polling_waits_while_authorization_is_pending_then_returns_tokens() {
    let server = MockServer::start().await;
    mount_device_code(&server).await;
    token_endpoint()
        .respond_with(oauth_error("authorization_pending"))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    token_endpoint()
        .and(body_string_contains(
            "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code",
        ))
        .and(body_string_contains("device_code=dev-code"))
        .and(body_string_contains("client_id=client-123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_type": "Bearer",
            "access_token": "access-1",
            "refresh_token": "refresh-1",
            "expires_in": 3600
        })))
        .mount(&server)
        .await;
    let client = client(&server);
    let code = client.start_device_code().await.unwrap();
    let started = tokio::time::Instant::now();

    let tokens = client.poll_device_code(&code).await.unwrap();

    assert_eq!(tokens.access_token, "access-1");
    assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-1"));
    assert!(tokens.expires_at > jiff::Timestamp::now());
    assert!(started.elapsed() >= Duration::from_secs(15));
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
