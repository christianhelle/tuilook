//! Entra ID sign-in: OAuth2 device-code flow, token refresh and storage.

mod oauth;
mod secrets;

pub use oauth::{AuthError, DeviceCode, OAuthClient, OAuthConfig, Sleeper, TokenSet};
pub use secrets::{KeyringStore, MemoryStore, SecretError, SecretStore};
