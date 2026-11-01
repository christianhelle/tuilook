mod support;

use serde_json::json;
use tuilook::graph::GraphBackend;
use tuilook::model::{Attachment, AttachmentId, AttachmentKind, MailBackend, MessageId};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
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
