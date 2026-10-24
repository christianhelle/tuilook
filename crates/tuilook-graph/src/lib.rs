//! Microsoft Graph v1.0 mail client and its [`MailBackend`](tuilook_model::MailBackend) impl.

mod client;

pub use client::{GRAPH_V1, GraphClient, GraphError, RetryPolicy};
