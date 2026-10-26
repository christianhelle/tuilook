//! Entra ID sign-in: OAuth2 device-code flow, token refresh and storage.

mod http;
mod oauth;
mod provider;
mod secrets;

pub use http::{USER_AGENT, http_client};
pub use oauth::{AuthError, DeviceCode, OAuthClient, OAuthConfig, Sleeper, TokenSet};
pub use provider::{TokenProvider, TokenSource};
pub use secrets::{KeyringStore, MemoryStore, SecretError, SecretStore};
