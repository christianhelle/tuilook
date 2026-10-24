use std::{future::Future, pin::Pin, sync::Arc, time::Duration};

use jiff::Timestamp;
use serde::Deserialize;

/// Where and as whom to sign in.
#[derive(Debug, Clone)]
pub struct OAuthConfig {
    /// e.g. `https://login.microsoftonline.com`
    pub authority: String,
    /// `common`, `organizations`, `consumers` or a tenant id.
    pub tenant: String,
    pub client_id: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("not signed in; run `tuilook login`")]
    NotSignedIn,
    #[error(transparent)]
    Secrets(#[from] crate::SecretError),
    #[error("sign-in was declined")]
    Declined,
    #[error("the sign-in code expired before it was used")]
    Expired,
    #[error("{error}: {description}")]
    OAuth { error: String, description: String },
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

/// What the user needs to finish signing in on another device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    pub message: String,
    pub interval: Duration,
    device_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Timestamp,
}

/// Waits between polls. Swapped out in tests so they need not wait.
pub type Sleeper = Arc<dyn Fn(Duration) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

pub struct OAuthClient {
    http: reqwest::Client,
    config: OAuthConfig,
    sleep: Sleeper,
}

impl OAuthClient {
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            sleep: Arc::new(|d| Box::pin(tokio::time::sleep(d))),
        }
    }

    pub fn with_sleeper(mut self, sleep: Sleeper) -> Self {
        self.sleep = sleep;
        self
    }

    /// Asks Entra ID for a code the user enters at the verification URL.
    pub async fn start_device_code(&self) -> Result<DeviceCode, AuthError> {
        let scope = self.config.scopes.join(" ");
        let response = self
            .http
            .post(self.endpoint("devicecode"))
            .form(&[
                ("client_id", self.config.client_id.as_str()),
                ("scope", &scope),
            ])
            .send()
            .await?;
        let body: DeviceCodeResponse = parse(response).await?;
        Ok(DeviceCode {
            user_code: body.user_code,
            verification_uri: body.verification_uri,
            message: body.message,
            interval: Duration::from_secs(body.interval),
            device_code: body.device_code,
        })
    }

    /// Polls until the user approves, declines, or the code expires.
    pub async fn poll_device_code(&self, code: &DeviceCode) -> Result<TokenSet, AuthError> {
        let mut interval = code.interval;
        loop {
            (self.sleep)(interval).await;
            let result = self
                .request_tokens(&[
                    ("grant_type", DEVICE_CODE_GRANT),
                    ("client_id", &self.config.client_id),
                    ("device_code", &code.device_code),
                ])
                .await;
            match result {
                Err(AuthError::OAuth { error, description }) => match error.as_str() {
                    "authorization_pending" => {}
                    "slow_down" => interval += SLOW_DOWN_STEP,
                    "authorization_declined" => return Err(AuthError::Declined),
                    "expired_token" => return Err(AuthError::Expired),
                    _ => return Err(AuthError::OAuth { error, description }),
                },
                other => return other,
            }
        }
    }

    /// Trades a refresh token for a fresh access token.
    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet, AuthError> {
        let scope = self.config.scopes.join(" ");
        self.request_tokens(&[
            ("grant_type", "refresh_token"),
            ("client_id", &self.config.client_id),
            ("refresh_token", refresh_token),
            ("scope", &scope),
        ])
        .await
    }

    async fn request_tokens(&self, form: &[(&str, &str)]) -> Result<TokenSet, AuthError> {
        let response = self
            .http
            .post(self.endpoint("token"))
            .form(form)
            .send()
            .await?;
        let body: TokenResponse = parse(response).await?;
        Ok(TokenSet {
            access_token: body.access_token,
            refresh_token: body.refresh_token,
            expires_at: Timestamp::now() + Duration::from_secs(body.expires_in),
        })
    }

    fn endpoint(&self, name: &str) -> String {
        format!(
            "{}/{}/oauth2/v2.0/{name}",
            self.config.authority.trim_end_matches('/'),
            self.config.tenant
        )
    }
}

/// RFC 8628 section 3.5: back off by five seconds on `slow_down`.
const SLOW_DOWN_STEP: Duration = Duration::from_secs(5);

const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

#[derive(Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
    message: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// Decodes a successful response, or the OAuth error a failed one carries.
async fn parse<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, AuthError> {
    if response.status().is_success() {
        return Ok(response.json().await?);
    }
    let status = response.error_for_status_ref().map(|_| ());
    match response.json::<ErrorResponse>().await {
        Ok(e) => Err(AuthError::OAuth {
            error: e.error,
            description: e.error_description,
        }),
        Err(_) => Err(status.unwrap_err().into()),
    }
}
