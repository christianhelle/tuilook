mod support;

use std::{sync::atomic::Ordering, time::Duration};

use serde_json::{Value, json};
use tuilook_graph::{GraphError, RetryPolicy};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

#[tokio::test]
async fn get_sends_the_bearer_token_and_decodes_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .and(header("authorization", "Bearer token-1"))
        .and(header("prefer", "odata.maxpagesize=50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "me"})))
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);

    let me: Value = client
        .get("/me", &[("Prefer", "odata.maxpagesize=50")])
        .await
        .unwrap();

    assert_eq!(me, json!({"id": "me"}));
}

fn ok() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"ok": true}))
}

fn graph_error(status: u16, code: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({"error": {"code": code, "message": format!("{code} happened")}}))
}

#[tokio::test]
async fn throttled_requests_wait_for_retry_after_then_retry() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(graph_error(429, "TooManyRequests").insert_header("Retry-After", "7"))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path("/me"))
        .respond_with(ok())
        .mount(&server)
        .await;
    let (client, _, waits) = support::client(&server);

    let body: Value = client.get("/me", &[]).await.unwrap();

    assert_eq!(body, json!({"ok": true}));
    assert_eq!(waits.recorded(), [Duration::from_secs(7)]);
}

#[tokio::test]
async fn server_errors_back_off_exponentially_with_jitter() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(graph_error(503, "ServiceUnavailable"))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path("/me"))
        .respond_with(ok())
        .mount(&server)
        .await;
    let (client, _, waits) = support::client(&server);

    let _: Value = client.get("/me", &[]).await.unwrap();

    let waits = waits.recorded();
    assert_eq!(waits.len(), 2);
    // base 1s doubled per attempt, plus up to one base of jitter
    assert!(
        (Duration::from_secs(1)..Duration::from_secs(2)).contains(&waits[0]),
        "{waits:?}"
    );
    assert!(
        (Duration::from_secs(2)..Duration::from_secs(3)).contains(&waits[1]),
        "{waits:?}"
    );
}

#[tokio::test]
async fn retries_give_up_after_the_attempt_limit() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(graph_error(503, "ServiceUnavailable"))
        .expect(3)
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);
    let client = client.with_retry(RetryPolicy {
        max_attempts: 3,
        ..Default::default()
    });

    let error = client.get::<Value>("/me", &[]).await.unwrap_err();

    assert!(
        matches!(&error, GraphError::Status { status, code, .. }
            if status.as_u16() == 503 && code == "ServiceUnavailable"),
        "{error:?}"
    );
}

#[tokio::test]
async fn client_errors_are_not_retried() {
    let server = MockServer::start().await;
    Mock::given(path("/me/messages/nope"))
        .respond_with(graph_error(404, "ErrorItemNotFound"))
        .expect(1)
        .mount(&server)
        .await;
    let (client, _, waits) = support::client(&server);

    let error = client
        .get::<Value>("/me/messages/nope", &[])
        .await
        .unwrap_err();

    assert!(
        matches!(&error, GraphError::Status { code, .. } if code == "ErrorItemNotFound"),
        "{error:?}"
    );
    assert!(waits.recorded().is_empty());
}

#[tokio::test]
async fn an_unauthorized_response_refreshes_the_token_and_retries_once() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .and(header("authorization", "Bearer token-1"))
        .respond_with(graph_error(401, "InvalidAuthenticationToken"))
        .mount(&server)
        .await;
    Mock::given(path("/me"))
        .and(header("authorization", "Bearer token-2"))
        .respond_with(ok())
        .mount(&server)
        .await;
    let (client, tokens, _) = support::client(&server);

    let body: Value = client.get("/me", &[]).await.unwrap();

    assert_eq!(body, json!({"ok": true}));
    assert_eq!(tokens.invalidations.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_second_unauthorized_response_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(graph_error(401, "InvalidAuthenticationToken"))
        .expect(2)
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);

    let error = client.get::<Value>("/me", &[]).await.unwrap_err();

    assert!(
        matches!(&error, GraphError::Status { status, .. } if status.as_u16() == 401),
        "{error:?}"
    );
}

#[tokio::test]
async fn at_most_four_requests_are_in_flight() {
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(ok().set_delay(Duration::from_millis(200)))
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);
    let started = std::time::Instant::now();

    let requests = (0..8).map(|_| client.get::<Value>("/me", &[]));
    for result in futures::future::join_all(requests).await {
        result.unwrap();
    }

    // Eight 200 ms requests, four at a time, take two rounds.
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "{:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn a_body_that_does_not_match_says_which_field_is_wrong() {
    #[derive(Debug, serde::Deserialize)]
    #[allow(dead_code)]
    struct Me {
        id: String,
    }
    let server = MockServer::start().await;
    Mock::given(path("/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": null})))
        .mount(&server)
        .await;
    let (client, _, _) = support::client(&server);

    let error = client.get::<Me>("/me", &[]).await.unwrap_err();

    let message = error.to_string();
    assert!(message.contains("/me"), "{message}");
    assert!(
        message.contains("invalid type: null, expected a string"),
        "{message}"
    );
}
