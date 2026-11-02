use std::path::PathBuf;

use jiff::{SignedDuration, Timestamp};
use reqwest::Client;
use semver::Version;
use serde_json::json;
use tempfile::TempDir;
use tuilook::update::{Saved, available, check, latest};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const RELEASES: &str = "/repos/christianhelle/tuilook/releases/latest";

fn version(version: &str) -> Version {
    Version::parse(version).unwrap()
}

fn now() -> Timestamp {
    Timestamp::from_second(1_000_000).unwrap()
}

fn hours_ago(hours: i64) -> Timestamp {
    now() - SignedDuration::from_hours(hours)
}

fn release(tag: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({
        "tag_name": tag,
        "html_url": format!("https://github.com/christianhelle/tuilook/releases/tag/{tag}"),
    }))
}

/// A server that answers `expected` requests for the latest release.
async fn github(response: ResponseTemplate, expected: u64) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(RELEASES))
        .respond_with(response)
        .expect(expected)
        .mount(&server)
        .await;
    server
}

/// A place to save checks in, which is removed along with the `TempDir`.
fn saved_file() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("update-check.json");
    (dir, path)
}

fn save(path: &std::path::Path, checked_at: Timestamp, latest: &str) {
    Saved {
        checked_at,
        latest: version(latest),
    }
    .save(path)
    .unwrap();
}

#[tokio::test]
async fn the_latest_release_is_read_from_its_tag() {
    let server = github(release("v0.1.5"), 1).await;

    let release = latest(&Client::new(), &server.uri()).await.unwrap();

    assert_eq!(release.version, version("0.1.5"));
    assert_eq!(
        release.url,
        "https://github.com/christianhelle/tuilook/releases/tag/v0.1.5"
    );
}

#[tokio::test]
async fn a_tag_need_not_start_with_v() {
    let server = github(release("0.1.5"), 1).await;

    let release = latest(&Client::new(), &server.uri()).await.unwrap();

    assert_eq!(release.version, version("0.1.5"));
}

#[tokio::test]
async fn a_tag_that_is_not_a_version_is_an_error() {
    let server = github(release("nightly"), 1).await;

    assert!(latest(&Client::new(), &server.uri()).await.is_err());
}

#[tokio::test]
async fn a_refusal_from_github_is_an_error() {
    for status in [403, 404] {
        let server = github(ResponseTemplate::new(status), 1).await;

        assert!(latest(&Client::new(), &server.uri()).await.is_err());
    }
}

#[tokio::test]
async fn checking_remembers_the_answer() {
    let server = github(release("v0.1.5"), 1).await;
    let (_dir, file) = saved_file();

    check(&Client::new(), &server.uri(), &file, now())
        .await
        .unwrap();

    assert_eq!(
        Saved::load(&file),
        Some(Saved {
            checked_at: now(),
            latest: version("0.1.5"),
        })
    );
}

#[tokio::test]
async fn a_newer_release_is_reported() {
    let server = github(release("v0.1.5"), 1).await;
    let (_dir, file) = saved_file();

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, Some(version("0.1.5")));
}

#[tokio::test]
async fn the_running_release_is_not_reported() {
    let server = github(release("v0.1.4"), 1).await;
    let (_dir, file) = saved_file();

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, None);
}

#[tokio::test]
async fn github_is_asked_only_once_a_day() {
    let server = github(release("v0.1.5"), 1).await;
    let (_dir, file) = saved_file();
    let current = version("0.1.4");
    let client = Client::new();

    let first = available(&client, &server.uri(), &file, &current, now()).await;
    let second = available(&client, &server.uri(), &file, &current, now()).await;

    assert_eq!(first, Some(version("0.1.5")));
    assert_eq!(second, Some(version("0.1.5")));
}

#[tokio::test]
async fn a_recent_check_stands_without_asking() {
    let server = github(release("v9.9.9"), 0).await;
    let (_dir, file) = saved_file();
    save(&file, hours_ago(23), "0.1.5");

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, Some(version("0.1.5")));
}

#[tokio::test]
async fn a_saved_version_is_not_reported_once_it_is_running() {
    let server = github(release("v9.9.9"), 0).await;
    let (_dir, file) = saved_file();
    save(&file, hours_ago(1), "0.1.5");

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.5"),
        now(),
    )
    .await;

    assert_eq!(update, None);
}

#[tokio::test]
async fn a_stale_check_is_replaced() {
    let server = github(release("v0.1.6"), 1).await;
    let (_dir, file) = saved_file();
    save(&file, hours_ago(25), "0.1.5");

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, Some(version("0.1.6")));
    assert_eq!(
        Saved::load(&file),
        Some(Saved {
            checked_at: now(),
            latest: version("0.1.6"),
        })
    );
}

#[tokio::test]
async fn a_failed_check_falls_back_on_the_last_answer() {
    let server = github(ResponseTemplate::new(500), 1).await;
    let (_dir, file) = saved_file();
    save(&file, hours_ago(25), "0.1.5");

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, Some(version("0.1.5")));
    assert_eq!(
        Saved::load(&file).map(|saved| saved.checked_at),
        Some(hours_ago(25)),
        "a failed check should be tried again next time"
    );
}

#[tokio::test]
async fn a_failed_first_check_reports_nothing() {
    let server = github(ResponseTemplate::new(500), 1).await;
    let (_dir, file) = saved_file();

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, None);
}

#[tokio::test]
async fn a_corrupt_file_counts_as_never_having_checked() {
    let server = github(release("v0.1.5"), 1).await;
    let (_dir, file) = saved_file();
    std::fs::write(&file, "not json").unwrap();

    let update = available(
        &Client::new(),
        &server.uri(),
        &file,
        &version("0.1.4"),
        now(),
    )
    .await;

    assert_eq!(update, Some(version("0.1.5")));
    assert!(Saved::load(&file).is_some());
}
