mod support;

use serde_json::json;
use tuilook_graph::GraphBackend;
use tuilook_model::{ComposeKind, Draft, MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path},
};

fn draft(kind: ComposeKind) -> Draft {
    Draft {
        kind,
        to: vec!["ada@example.com".to_owned()],
        cc: vec![],
        subject: "RE: Engine".to_owned(),
        body: "Agreed.\nSee you <then> & there".to_owned(),
    }
}

#[tokio::test]
async fn a_new_message_is_sent_as_plain_text_and_kept_in_sent_items() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/sendMail"))
        .and(body_json(json!({
            "message": {
                "subject": "Hello",
                "body": { "contentType": "Text", "content": "Hi there" },
                "toRecipients": [{ "emailAddress": { "address": "ada@example.com" } }],
                "ccRecipients": [{ "emailAddress": { "address": "grace@example.com" } }],
            },
            "saveToSentItems": true,
        })))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend
        .send(&Draft {
            kind: ComposeKind::New,
            to: vec!["ada@example.com".to_owned()],
            cc: vec!["grace@example.com".to_owned()],
            subject: "Hello".to_owned(),
            body: "Hi there".to_owned(),
        })
        .await
        .unwrap();
}

/// Graph drafts the reply, tuilook fills it in, then sends it.
async fn mount_reply(server: &MockServer, create: &str, drafted: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path(format!("/me/messages/m1/{create}")))
        .respond_with(ResponseTemplate::new(201).set_body_json(drafted))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/me/messages/d1/send"))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_reply_puts_the_text_above_the_quoted_original() {
    let server = MockServer::start().await;
    mount_reply(
        &server,
        "createReply",
        json!({
            "id": "d1",
            "body": { "contentType": "html", "content": "<html><body dir=\"ltr\"><p>quoted</p></body></html>" },
            "toRecipients": [{ "emailAddress": { "address": "ada@example.com" } }],
        }),
    )
    .await;
    Mock::given(method("PATCH"))
        .and(path("/me/messages/d1"))
        .and(body_json(json!({
            "subject": "RE: Engine",
            "body": {
                "contentType": "HTML",
                "content": "<html><body dir=\"ltr\"><div>Agreed.<br>See you &lt;then&gt; &amp; there</div><p>quoted</p></body></html>",
            },
            "toRecipients": [{ "emailAddress": { "address": "ada@example.com" } }],
            "ccRecipients": [],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "d1" })))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend
        .send(&draft(ComposeKind::Reply(MessageId("m1".to_owned()))))
        .await
        .unwrap();
}

#[tokio::test]
async fn reply_all_keeps_everyone_graph_picked_and_adds_the_typed_addresses() {
    let server = MockServer::start().await;
    mount_reply(
        &server,
        "createReplyAll",
        json!({
            "id": "d1",
            "body": { "contentType": "text", "content": "> quoted" },
            "toRecipients": [{ "emailAddress": { "address": "Ada@example.com" } }],
            "ccRecipients": [{ "emailAddress": { "address": "grace@example.com" } }],
        }),
    )
    .await;
    Mock::given(method("PATCH"))
        .and(path("/me/messages/d1"))
        .and(body_json(json!({
            "subject": "RE: Engine",
            "body": { "contentType": "Text", "content": "Agreed.\nSee you <then> & there\n\n> quoted" },
            "toRecipients": [{ "emailAddress": { "address": "Ada@example.com" } }],
            "ccRecipients": [
                { "emailAddress": { "address": "grace@example.com" } },
                { "emailAddress": { "address": "alan@example.com" } },
            ],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "d1" })))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);
    let mut reply_all = draft(ComposeKind::ReplyAll(MessageId("m1".to_owned())));
    reply_all.cc = vec!["alan@example.com".to_owned()];

    backend.send(&reply_all).await.unwrap();
}

#[tokio::test]
async fn a_forward_goes_to_the_typed_addresses() {
    let server = MockServer::start().await;
    mount_reply(
        &server,
        "createForward",
        json!({
            "id": "d1",
            "body": { "contentType": "html", "content": "<p>original</p>" },
        }),
    )
    .await;
    Mock::given(method("PATCH"))
        .and(path("/me/messages/d1"))
        .and(body_json(json!({
            "subject": "RE: Engine",
            "body": {
                "contentType": "HTML",
                "content": "<div>Agreed.<br>See you &lt;then&gt; &amp; there</div><p>original</p>",
            },
            "toRecipients": [{ "emailAddress": { "address": "ada@example.com" } }],
            "ccRecipients": [],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "d1" })))
        .expect(1)
        .mount(&server)
        .await;
    let backend = GraphBackend::new(support::client(&server).0);

    backend
        .send(&draft(ComposeKind::Forward(MessageId("m1".to_owned()))))
        .await
        .unwrap();
}
