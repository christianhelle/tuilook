//! Microsoft Graph v1.0 mail client and its [`MailBackend`](tuilook_model::MailBackend) impl.

mod backend;
mod client;
mod dto;

pub use backend::GraphBackend;
pub use client::{GRAPH_V1, GraphClient, GraphError, RetryPolicy};
