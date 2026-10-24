//! Entra ID sign-in: OAuth2 device-code flow, token refresh and storage.

mod oauth;

pub use oauth::{AuthError, DeviceCode, OAuthClient, OAuthConfig, TokenSet};
