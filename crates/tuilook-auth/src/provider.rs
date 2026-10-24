use std::sync::Arc;

use jiff::{SignedDuration, Timestamp};
use tokio::sync::Mutex;

use crate::{AuthError, DeviceCode, OAuthClient, SecretStore, TokenSet};

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
        let tokens = match self.oauth.refresh(&refresh_token).await {
            Err(AuthError::OAuth { error, .. }) if error == "invalid_grant" => {
                self.secrets.clear()?;
                return Err(AuthError::NotSignedIn);
            }
            result => result?,
        };
        if let Some(rotated) = &tokens.refresh_token {
            self.secrets.save(rotated)?;
        }
        let access_token = tokens.access_token.clone();
        *cached = Some(tokens);
        Ok(access_token)
    }

    /// Signs in with the device-code flow. `show_code` tells the user where
    /// to go and what to enter.
    pub async fn login(&self, show_code: impl FnOnce(&DeviceCode)) -> Result<(), AuthError> {
        let code = self.oauth.start_device_code().await?;
        show_code(&code);
        let tokens = self.oauth.poll_device_code(&code).await?;
        if let Some(refresh_token) = &tokens.refresh_token {
            self.secrets.save(refresh_token)?;
        }
        *self.cached.lock().await = Some(tokens);
        Ok(())
    }

    /// Forgets the cached access token, e.g. after Graph answered 401.
    pub async fn invalidate(&self) {
        *self.cached.lock().await = None;
    }
}

/// Something that hands out Graph access tokens.
pub trait TokenSource: Send + Sync {
    fn access_token(&self) -> impl Future<Output = Result<String, AuthError>> + Send;

    /// The last token was rejected; the next call must fetch a new one.
    fn invalidate(&self) -> impl Future<Output = ()> + Send;
}

impl TokenSource for TokenProvider {
    async fn access_token(&self) -> Result<String, AuthError> {
        TokenProvider::access_token(self).await
    }

    async fn invalidate(&self) {
        TokenProvider::invalidate(self).await
    }
}

impl<T: TokenSource> TokenSource for Arc<T> {
    async fn access_token(&self) -> Result<String, AuthError> {
        (**self).access_token().await
    }

    async fn invalidate(&self) {
        (**self).invalidate().await
    }
}
