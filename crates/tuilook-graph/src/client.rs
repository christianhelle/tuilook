use std::{sync::Arc, time::Duration};

use reqwest::StatusCode;
use serde::de::DeserializeOwned;
use tokio::sync::Semaphore;
use tuilook_auth::{AuthError, Sleeper, TokenSource};

pub const GRAPH_V1: &str = "https://graph.microsoft.com/v1.0";

/// Requests in flight at once; Graph throttles mailboxes beyond about 4.
const MAX_CONCURRENT: usize = 4;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("Graph returned {status}: {code}: {message}")]
    Status {
        status: StatusCode,
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
        }
    }
}

pub struct GraphClient<T> {
    http: reqwest::Client,
    base_url: String,
    tokens: T,
    limit: Semaphore,
    retry: RetryPolicy,
    sleep: Sleeper,
}

impl<T: TokenSource> GraphClient<T> {
    pub fn new(base_url: impl Into<String>, tokens: T) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            tokens,
            limit: Semaphore::new(MAX_CONCURRENT),
            retry: RetryPolicy::default(),
            sleep: Arc::new(|d| Box::pin(tokio::time::sleep(d))),
        }
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_sleeper(mut self, sleep: Sleeper) -> Self {
        self.sleep = sleep;
        self
    }

    /// GETs `path` (relative to the base URL, or an absolute `@odata.nextLink`).
    pub async fn get<R: DeserializeOwned>(
        &self,
        path: &str,
        headers: &[(&str, &str)],
    ) -> Result<R, GraphError> {
        let url = if path.starts_with("http") {
            path.to_owned()
        } else {
            format!("{}{path}", self.base_url.trim_end_matches('/'))
        };
        let mut attempt = 0;
        let mut reauthenticated = false;
        loop {
            attempt += 1;
            let response = self.send(&url, headers).await?;
            let status = response.status();
            if status.is_success() {
                return Ok(response.json().await?);
            }
            if status == StatusCode::UNAUTHORIZED && !reauthenticated {
                reauthenticated = true;
                self.tokens.invalidate().await;
                continue;
            }
            if !is_retryable(status) || attempt >= self.retry.max_attempts {
                return Err(status_error(response).await);
            }
            let wait = retry_after(&response).unwrap_or_else(|| self.backoff(attempt));
            (self.sleep)(wait).await;
        }
    }

    async fn send(
        &self,
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<reqwest::Response, GraphError> {
        let _permit = self
            .limit
            .acquire()
            .await
            .expect("semaphore is never closed");
        let token = self.tokens.access_token().await?;
        let mut request = self.http.get(url).bearer_auth(token);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        Ok(request.send().await?)
    }

    /// Exponential backoff for the given 1-based attempt, plus jitter.
    fn backoff(&self, attempt: u32) -> Duration {
        let base = self.retry.base_delay;
        let exponential = base.saturating_mul(1 << (attempt - 1).min(16));
        let jitter = base.mul_f64(fastrand::f64());
        (exponential + jitter).min(self.retry.max_delay)
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (status.is_server_error() && status != StatusCode::NOT_IMPLEMENTED)
}

fn retry_after(response: &reqwest::Response) -> Option<Duration> {
    let seconds = response.headers().get("retry-after")?.to_str().ok()?;
    Some(Duration::from_secs(seconds.trim().parse().ok()?))
}

#[derive(serde::Deserialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(serde::Deserialize)]
struct ErrorDetail {
    code: String,
    #[serde(default)]
    message: String,
}

async fn status_error(response: reqwest::Response) -> GraphError {
    let status = response.status();
    let (code, message) = match response.json::<ErrorBody>().await {
        Ok(body) => (body.error.code, body.error.message),
        Err(_) => (String::new(), String::new()),
    };
    GraphError::Status {
        status,
        code,
        message,
    }
}
