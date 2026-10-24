use std::sync::Arc;

use jiff::{SignedDuration, Timestamp};
use tokio::sync::Mutex;

use crate::{AuthError, OAuthClient, SecretStore, TokenSet};

/// Refresh this long before the access token actually expires.
const EXPIRY_MARGIN: SignedDuration = SignedDuration::from_mins(5);

/// Hands out a valid access token, refreshing it as needed. The access
/// token stays in memory; only the refresh token is persisted.
pub struct TokenProvider {
    oauth: OAuthClient,
    secrets: Arc<dyn SecretStore>,
    cached: Mutex<Option<TokenSet>>,
}

impl TokenProvider {
    pub fn new(oauth: OAuthClient, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            oauth,
            secrets,
            cached: Mutex::new(None),
        }
    }

    pub async fn access_token(&self) -> Result<String, AuthError> {
        // Held across the refresh so concurrent callers share one request.
        let mut cached = self.cached.lock().await;
        if let Some(tokens) = cached.as_ref()
            && tokens.expires_at - EXPIRY_MARGIN > Timestamp::now()
        {
            return Ok(tokens.access_token.clone());
        }
        let refresh_token = self.secrets.load()?.ok_or(AuthError::NotSignedIn)?;
        let tokens = self.oauth.refresh(&refresh_token).await?;
        if let Some(rotated) = &tokens.refresh_token {
            self.secrets.save(rotated)?;
        }
        let access_token = tokens.access_token.clone();
        *cached = Some(tokens);
        Ok(access_token)
    }
}
