use serde_json::json;
use tuilook_auth::{AuthError, OAuthClient, OAuthConfig};
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
async fn refresh_exchanges_a_refresh_token_for_new_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=refresh-1"))
        .and(body_string_contains("client_id=client-123"))
        .and(body_string_contains("scope=offline_access+Mail.Read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_type": "Bearer",
            "access_token": "access-2",
            "refresh_token": "refresh-2",
            "expires_in": 3600
        })))
        .expect(1)
        .mount(&server)
        .await;

    let tokens = client(&server).refresh("refresh-1").await.unwrap();

    assert_eq!(tokens.access_token, "access-2");
    assert_eq!(tokens.refresh_token.as_deref(), Some("refresh-2"));
}

#[tokio::test]
async fn a_rejected_refresh_token_reports_the_oauth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/common/oauth2/v2.0/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({
            "error": "invalid_grant",
            "error_description": "AADSTS70008: The refresh token has expired."
        })))
        .mount(&server)
        .await;

    let error = client(&server).refresh("stale").await.unwrap_err();

    assert!(
        matches!(&error, AuthError::OAuth { error, .. } if error == "invalid_grant"),
        "{error:?}"
    );
}
