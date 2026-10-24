#![allow(dead_code)]

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use tuilook_auth::{AuthError, Sleeper, TokenSource};
use tuilook_graph::GraphClient;
use wiremock::MockServer;

/// Hands out `token-1`, then `token-2` after each invalidation.
#[derive(Default)]
pub struct FakeTokens {
    pub invalidations: AtomicUsize,
}

impl TokenSource for FakeTokens {
    async fn access_token(&self) -> Result<String, AuthError> {
        Ok(format!(
            "token-{}",
            self.invalidations.load(Ordering::SeqCst) + 1
        ))
    }

    async fn invalidate(&self) {
        self.invalidations.fetch_add(1, Ordering::SeqCst);
    }
}

pub struct Waits(pub Arc<Mutex<Vec<Duration>>>);

impl Waits {
    pub fn recorded(&self) -> Vec<Duration> {
        self.0.lock().unwrap().clone()
    }
}

/// A client against `server` whose waits return at once and are recorded.
pub fn client(server: &MockServer) -> (GraphClient<Arc<FakeTokens>>, Arc<FakeTokens>, Waits) {
    let tokens = Arc::new(FakeTokens::default());
    let waits = Arc::new(Mutex::new(Vec::new()));
    let recorded = waits.clone();
    let sleeper: Sleeper = Arc::new(move |d| {
        recorded.lock().unwrap().push(d);
        Box::pin(async {})
    });
    let client = GraphClient::new(server.uri(), tokens.clone()).with_sleeper(sleeper);
    (client, tokens, Waits(waits))
}
