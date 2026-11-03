mod support;

use std::sync::Arc;

use jiff::tz::TimeZone;
use serde_json::{Value, json};
use tuilook::graph::GraphBackend;
use tuilook::model::{AppointmentId, Invite, MailBackend, MessageId, Rsvp};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

fn backend(server: &MockServer) -> GraphBackend<Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
        .with_time_zone(TimeZone::get("Europe/Copenhagen").unwrap())
}

/// A meeting request as Graph sends it, its event in Copenhagen time.
fn meeting_request() -> Value {
    json!({
        "@odata.type": "#microsoft.graph.eventMessageRequest",
        "id": "m1",
        "event": {
            "id": "e1",
            "start": { "dateTime": "2026-11-10T14:00:00.0000000", "timeZone": "Europe/Copenhagen" },
            "end": { "dateTime": "2026-11-10T15:00:00.0000000", "timeZone": "Europe/Copenhagen" },
            "isAllDay": false,
            "location": { "displayName": "Room 4" },
            "organizer": { "emailAddress": { "name": "Ada Lovelace", "address": "ada@example.com" } },
            "responseStatus": { "response": "tentativelyAccepted" },
            "responseRequested": false,
        },
    })
}

#[tokio::test]
async fn a_meeting_request_is_an_invite_in_local_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/messages/m1"))
        .and(query_param(
            "$expand",
            "microsoft.graph.eventMessage/event($select=id,start,end,isAllDay,location,organizer,responseStatus,responseRequested)",
        ))
        .and(header("prefer", r#"outlook.timezone="Europe/Copenhagen""#))
        .respond_with(ResponseTemplate::new(200).set_body_json(meeting_request()))
        .expect(1)
        .mount(&server)
        .await;

    let invite = backend(&server)
        .invite(&MessageId("m1".to_owned()))
        .await
        .unwrap();

    assert_eq!(
        invite,
        Some(Invite {
            event: AppointmentId("e1".to_owned()),
            start: "2026-11-10T13:00:00Z".parse().unwrap(),
            end: "2026-11-10T14:00:00Z".parse().unwrap(),
            all_day: false,
            location: "Room 4".to_owned(),
            organizer: "Ada Lovelace".to_owned(),
            answer: Some(Rsvp::Tentative),
            response_requested: false,
        })
    );
}
