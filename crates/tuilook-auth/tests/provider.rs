use std::sync::Arc;

use serde_json::json;
use tuilook_auth::{AuthError, MemoryStore, OAuthClient, OAuthConfig, SecretStore, TokenProvider};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path},
};

fn provider(server: &MockServer, secrets: Arc<MemoryStore>) -> TokenProvider {
    let oauth = OAuthClient::new(OAuthConfig {
        authority: server.uri(),
        tenant: "common".to_owned(),
        client_id: "client-123".to_owned(),
        scopes: vec!["offline_access".to_owned()],
    });
    TokenProvider::new(oauth, secrets)
}

fn signed_in(refresh_token: &str) -> Arc<MemoryStore> {
    let secrets = Arc::new(MemoryStore::default());
    secrets.save(refresh_token).unwrap();
    secrets
}

fn tokens(access: &str, refresh: &str, expires_in: u64) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "token_type": "Bearer",
        "access_token": access,
        "refresh_token": refresh,
        "expires_in": expires_in
    }))
}

fn refresh_with(refresh_token: &str) -> wiremock::MockBuilder {
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/token"))
        .and(body_string_contains(format!(
            "refresh_token={refresh_token}"
        )))
}

#[tokio::test]
async fn without_a_stored_refresh_token_the_user_is_not_signed_in() {
    let server = MockServer::start().await;
    let provider = provider(&server, Arc::new(MemoryStore::default()));

    let error = provider.access_token().await.unwrap_err();

    assert!(matches!(error, AuthError::NotSignedIn), "{error:?}");
}

#[tokio::test]
async fn the_access_token_is_refreshed_once_then_reused() {
    let server = MockServer::start().await;
    refresh_with("refresh-1")
        .respond_with(tokens("access-1", "refresh-2", 3600))
        .expect(1)
        .mount(&server)
        .await;
    let provider = provider(&server, signed_in("refresh-1"));

    assert_eq!(provider.access_token().await.unwrap(), "access-1");
    assert_eq!(provider.access_token().await.unwrap(), "access-1");
}

#[tokio::test]
async fn a_rotated_refresh_token_is_saved() {
    let server = MockServer::start().await;
    refresh_with("refresh-1")
        .respond_with(tokens("access-1", "refresh-2", 3600))
        .mount(&server)
        .await;
    let secrets = signed_in("refresh-1");
    let provider = provider(&server, secrets.clone());

    provider.access_token().await.unwrap();

    assert_eq!(secrets.load().unwrap().as_deref(), Some("refresh-2"));
}

#[tokio::test]
async fn a_token_close_to_expiry_is_refreshed() {
    let server = MockServer::start().await;
    refresh_with("refresh-1")
        .respond_with(tokens("access-1", "refresh-2", 60))
        .mount(&server)
        .await;
    refresh_with("refresh-2")
        .respond_with(tokens("access-2", "refresh-3", 3600))
        .mount(&server)
        .await;
    let provider = provider(&server, signed_in("refresh-1"));

    assert_eq!(provider.access_token().await.unwrap(), "access-1");
    assert_eq!(provider.access_token().await.unwrap(), "access-2");
}

#[tokio::test]
async fn invalidating_forces_the_next_call_to_refresh() {
    let server = MockServer::start().await;
    refresh_with("refresh-1")
        .respond_with(tokens("access-1", "refresh-2", 3600))
        .mount(&server)
        .await;
    refresh_with("refresh-2")
        .respond_with(tokens("access-2", "refresh-3", 3600))
        .mount(&server)
        .await;
    let provider = provider(&server, signed_in("refresh-1"));
    provider.access_token().await.unwrap();

    provider.invalidate().await;

    assert_eq!(provider.access_token().await.unwrap(), "access-2");
}

#[tokio::test]
async fn a_rejected_refresh_token_signs_the_user_out() {
    let server = MockServer::start().await;
    refresh_with("revoked")
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant",
            "error_description": "AADSTS50173: The provided grant has expired."
        })))
        .mount(&server)
        .await;
    let secrets = signed_in("revoked");
    let provider = provider(&server, secrets.clone());

    let error = provider.access_token().await.unwrap_err();

    assert!(matches!(error, AuthError::NotSignedIn), "{error:?}");
    assert_eq!(secrets.load().unwrap(), None);
}
