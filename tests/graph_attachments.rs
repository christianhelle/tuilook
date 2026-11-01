mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use tuilook::model::{
    Attachment, AttachmentId, AttachmentKind, BackendError, ComposeKind, Draft, MailBackend,
    MessageId,
};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

#[tokio::test]
async fn attachments_leave_out_pictures_shown_in_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/messages/m1/attachments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "value": [
                {
                    "@odata.type": "#microsoft.graph.fileAttachment",
                    "id": "a1", "name": "report.pdf", "contentType": "application/pdf",
                    "size": 1200000, "isInline": false,
                },
                {
                    "@odata.type": "#microsoft.graph.fileAttachment",
                    "id": "a2", "name": "logo.png", "contentType": "image/png",
                    "size": 900, "isInline": true,
                },
                {
                    "@odata.type": "#microsoft.graph.itemAttachment",
                    "id": "a3", "name": "Minutes", "contentType": null,
                    "size": 4000, "isInline": false,
                },
                {
                    "@odata.type": "#microsoft.graph.referenceAttachment",
                    "id": "a4", "name": "Budget.xlsx", "size": 0, "isInline": false,
                },
            ],
        })))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    let attachments = backend
        .attachments(&MessageId("m1".to_owned()))
        .await
        .unwrap();

    let attachment = |id: &str, name: &str, content_type: &str, size, kind| Attachment {
        id: AttachmentId(id.to_owned()),
        name: name.to_owned(),
        content_type: content_type.to_owned(),
        size,
        kind,
    };
    assert_eq!(
        attachments,
        [
            attachment(
                "a1",
                "report.pdf",
                "application/pdf",
                1_200_000,
                AttachmentKind::File
            ),
            attachment("a3", "Minutes", "", 4_000, AttachmentKind::Item),
            attachment("a4", "Budget.xlsx", "", 0, AttachmentKind::Reference),
        ]
    );
}

#[tokio::test]
async fn an_attachment_downloads_as_its_raw_bytes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/messages/m1/attachments/a1/$value"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8, 159, 146, 150]))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    let bytes = backend
        .attachment_content(&MessageId("m1".to_owned()), &AttachmentId("a1".to_owned()))
        .await
        .unwrap();

    assert_eq!(bytes, [0, 159, 146, 150]);
}

/// A file called `name` holding `content`, in a folder kept while the test
/// runs.
fn file(name: &str, content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    std::fs::write(&path, content).unwrap();
    (dir, path)
}

fn new_message(attachments: Vec<std::path::PathBuf>) -> Draft {
    Draft {
        kind: ComposeKind::New,
        to: vec!["ada@example.com".to_owned()],
        cc: vec![],
        subject: "Notes".to_owned(),
        body: "Attached".to_owned(),
        attachments,
    }
}

async fn expect(server: &MockServer, verb: &str, at: &str, status: u16, times: u64) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({ "id": "d1" })))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_new_message_with_a_file_is_drafted_given_the_file_then_sent() {
    let server = MockServer::start().await;
    let (_dir, notes) = file("notes.txt", b"hello");
    Mock::given(method("POST"))
        .and(path("/me/messages"))
        .and(body_json(json!({
            "subject": "Notes",
            "body": { "contentType": "Text", "content": "Attached" },
            "toRecipients": [{ "emailAddress": { "address": "ada@example.com" } }],
            "ccRecipients": [],
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "d1" })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/me/messages/d1/attachments"))
        .and(body_json(json!({
            "@odata.type": "#microsoft.graph.fileAttachment",
            "name": "notes.txt",
            "contentBytes": "aGVsbG8=",
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "a1" })))
        .expect(1)
        .mount(&server)
        .await;
    expect(&server, "POST", "/me/messages/d1/send", 202, 1).await;
    expect(&server, "POST", "/me/sendMail", 202, 0).await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend.send(&new_message(vec![notes])).await.unwrap();
}

/// Whether a request carries no access token, as upload URLs want.
fn unsigned(request: &wiremock::Request) -> bool {
    !request.headers.contains_key("authorization")
}

#[tokio::test]
async fn a_file_over_3_mb_is_uploaded_in_chunks_without_the_access_token() {
    let server = MockServer::start().await;
    let size = 5 * 1024 * 1024;
    let (_dir, video) = file("demo.mp4", &vec![7u8; size]);
    expect(&server, "POST", "/me/messages", 201, 1).await;
    Mock::given(method("POST"))
        .and(path("/me/messages/d1/attachments/createUploadSession"))
        .and(body_json(json!({
            "AttachmentItem": {
                "attachmentType": "file",
                "name": "demo.mp4",
                "size": size,
            },
        })))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({ "uploadUrl": format!("{}/upload/s1", server.uri()) })),
        )
        .expect(1)
        .mount(&server)
        .await;
    for (range, status) in [
        ("bytes 0-3932159/5242880", 200),
        ("bytes 3932160-5242879/5242880", 201),
    ] {
        Mock::given(method("PUT"))
            .and(path("/upload/s1"))
            .and(header("Content-Range", range))
            .and(unsigned)
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
    }
    expect(&server, "POST", "/me/messages/d1/attachments", 201, 0).await;
    expect(&server, "POST", "/me/messages/d1/send", 202, 1).await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend.send(&new_message(vec![video])).await.unwrap();
}

#[tokio::test]
async fn a_file_over_150_mb_is_refused_before_anything_is_sent() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let huge = dir.path().join("backup.zip");
    std::fs::File::create(&huge)
        .unwrap()
        .set_len(150 * 1024 * 1024 + 1)
        .unwrap();
    expect(&server, "POST", "/me/messages", 201, 0).await;
    let backend = GraphBackend::new(support::client(&server).0);

    let sent = backend.send(&new_message(vec![huge])).await;

    assert_eq!(
        sent,
        Err(BackendError::Other(
            "backup.zip is over 150 MB, too big to attach".to_owned()
        ))
    );
}

#[tokio::test]
async fn a_file_that_will_not_attach_takes_its_draft_away_unsent() {
    let server = MockServer::start().await;
    let (_dir, notes) = file("notes.txt", b"hello");
    expect(&server, "POST", "/me/messages", 201, 1).await;
    expect(&server, "POST", "/me/messages/d1/attachments", 400, 1).await;
    expect(&server, "DELETE", "/me/messages/d1", 204, 1).await;
    expect(&server, "POST", "/me/messages/d1/send", 202, 0).await;
    let backend = GraphBackend::new(support::client(&server).0);

    let sent = backend.send(&new_message(vec![notes])).await;

    assert!(sent.is_err());
}

#[tokio::test]
async fn a_reply_carries_its_files_in_the_draft_graph_made() {
    let server = MockServer::start().await;
    let (_dir, notes) = file("notes.txt", b"hello");
    Mock::given(method("POST"))
        .and(path("/me/messages/m1/createReply"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({
            "id": "d1",
            "body": { "contentType": "text", "content": "> quoted" },
        })))
        .expect(1)
        .mount(&server)
        .await;
    expect(&server, "PATCH", "/me/messages/d1", 200, 1).await;
    expect(&server, "POST", "/me/messages/d1/attachments", 201, 1).await;
    expect(&server, "POST", "/me/messages/d1/send", 202, 1).await;
    let backend = GraphBackend::new(support::client(&server).0);
    let reply = Draft {
        kind: ComposeKind::Reply(MessageId("m1".to_owned())),
        ..new_message(vec![notes])
    };

    backend.send(&reply).await.unwrap();
}
