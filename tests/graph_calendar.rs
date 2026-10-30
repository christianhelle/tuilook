mod support;

use std::sync::Arc;

use jiff::{Timestamp, tz::TimeZone};
use serde_json::{Value, json};
use tuilook::graph::GraphBackend;
use tuilook::model::{Appointment, AppointmentDraft, AppointmentId, BackendError, MailBackend};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, headers, method, path, query_param},
};

fn backend(server: &MockServer) -> GraphBackend<Arc<support::FakeTokens>> {
    GraphBackend::new(support::client(server).0)
        .with_time_zone(TimeZone::get("Europe/Copenhagen").unwrap())
}

fn at(time: &str) -> Timestamp {
    time.parse().unwrap()
}

/// A Graph event as asked for in Copenhagen time.
fn graph_event(id: &str, subject: &str, start: &str, end: &str) -> Value {
    json!({
        "id": id,
        "subject": subject,
        "start": { "dateTime": format!("{start}.0000000"), "timeZone": "Europe/Copenhagen" },
        "end": { "dateTime": format!("{end}.0000000"), "timeZone": "Europe/Copenhagen" },
        "isAllDay": false,
        "location": { "displayName": "Room 4" },
        "organizer": { "emailAddress": { "name": "Ada Lovelace", "address": "ada@example.com" } },
        "attendees": [
            { "type": "required", "emailAddress": { "name": "Grace", "address": "grace@example.com" } },
        ],
        "bodyPreview": "Agenda inside",
        "isOrganizer": false,
    })
}

fn page(events: Vec<Value>, next: Option<String>) -> ResponseTemplate {
    let mut body = json!({ "value": events });
    if let Some(next) = next {
        body["@odata.nextLink"] = json!(next);
    }
    ResponseTemplate::new(200).set_body_json(body)
}

#[tokio::test]
async fn lists_the_appointments_in_a_window_across_pages_in_local_time() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/calendarView"))
        .and(query_param("startDateTime", "2026-11-01T00:00:00Z"))
        .and(query_param("endDateTime", "2026-12-01T00:00:00Z"))
        .and(headers(
            "prefer",
            vec![
                r#"outlook.timezone="Europe/Copenhagen""#,
                "odata.maxpagesize=50",
            ],
        ))
        .respond_with(page(
            vec![graph_event(
                "e1",
                "Standup",
                "2026-11-02T09:00:00",
                "2026-11-02T09:15:00",
            )],
            Some(format!("{}/me/calendarView/page2", server.uri())),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/me/calendarView/page2"))
        .respond_with(page(
            vec![graph_event(
                "e2",
                "Review",
                "2026-11-03T14:00:00",
                "2026-11-03T15:00:00",
            )],
            None,
        ))
        .mount(&server)
        .await;

    let appointments = backend(&server)
        .appointments(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
        .await
        .unwrap();

    assert_eq!(
        appointments[0],
        Appointment {
            id: AppointmentId("e1".to_owned()),
            subject: "Standup".to_owned(),
            // Copenhagen is an hour ahead of UTC in November.
            start: at("2026-11-02T08:00:00Z"),
            end: at("2026-11-02T08:15:00Z"),
            all_day: false,
            location: "Room 4".to_owned(),
            organizer: "Ada Lovelace".to_owned(),
            attendees: vec!["grace@example.com".to_owned()],
            preview: "Agenda inside".to_owned(),
            is_organizer: false,
        }
    );
    assert_eq!(appointments[1].id, AppointmentId("e2".to_owned()));
    assert_eq!(appointments[1].start, at("2026-11-03T13:00:00Z"));
}

#[tokio::test]
async fn an_all_day_appointment_runs_from_local_midnight_to_midnight() {
    let server = MockServer::start().await;
    let mut holiday = graph_event(
        "e1",
        "Holiday",
        "2026-11-05T00:00:00",
        "2026-11-06T00:00:00",
    );
    holiday["isAllDay"] = json!(true);
    Mock::given(method("GET"))
        .and(path("/me/calendarView"))
        .respond_with(page(vec![holiday], None))
        .mount(&server)
        .await;

    let appointments = backend(&server)
        .appointments(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
        .await
        .unwrap();

    assert!(appointments[0].all_day);
    assert_eq!(appointments[0].start, at("2026-11-04T23:00:00Z"));
    assert_eq!(appointments[0].end, at("2026-11-05T23:00:00Z"));
}

fn draft() -> AppointmentDraft {
    AppointmentDraft {
        subject: "Planning".to_owned(),
        start: at("2026-11-02T08:30:00Z"),
        end: at("2026-11-02T09:30:00Z"),
        all_day: false,
        location: "Room 4".to_owned(),
        attendees: vec!["grace@example.com".to_owned()],
        notes: "Bring the numbers".to_owned(),
    }
}

/// How Graph is sent `draft()`, in Copenhagen time.
fn draft_json() -> Value {
    json!({
        "subject": "Planning",
        "body": { "contentType": "Text", "content": "Bring the numbers" },
        "start": { "dateTime": "2026-11-02T09:30:00", "timeZone": "Europe/Copenhagen" },
        "end": { "dateTime": "2026-11-02T10:30:00", "timeZone": "Europe/Copenhagen" },
        "isAllDay": false,
        "location": { "displayName": "Room 4" },
        "attendees": [
            { "type": "required", "emailAddress": { "address": "grace@example.com" } },
        ],
    })
}

#[tokio::test]
async fn a_new_appointment_is_written_in_local_time() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/me/events"))
        .and(body_json(draft_json()))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({ "id": "e9" })))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server).create_appointment(&draft()).await.unwrap();
}

#[tokio::test]
async fn a_changed_appointment_is_patched_with_every_field() {
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path("/me/events/e1"))
        .and(body_json(draft_json()))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "id": "e1" })))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .update_appointment(&AppointmentId("e1".to_owned()), &draft())
        .await
        .unwrap();
}

#[tokio::test]
async fn deleting_an_appointment_deletes_the_event() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/me/events/e1"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    backend(&server)
        .delete_appointment(&AppointmentId("e1".to_owned()))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_sign_in_without_calendar_access_says_to_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me/calendarView"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "error": { "code": "ErrorAccessDenied", "message": "Access is denied." }
        })))
        .mount(&server)
        .await;

    let error = backend(&server)
        .appointments(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"))
        .await
        .unwrap_err();

    assert_eq!(
        error,
        BackendError::Other("sign in again (otlk login) to allow calendar access".to_owned())
    );
}
