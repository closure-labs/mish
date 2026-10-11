use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mish::{
    appliance::EventStore,
    coordinator::Coordinator,
    domain::Role,
    server::{AppState, router},
    storage::{MishStore, default_config},
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tower::ServiceExt;
async fn request(
    app: &Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(token) = token {
        request = request.header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    assert_eq!(response.headers()["cache-control"], "no-store");
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
fn setup() -> (tempfile::TempDir, Coordinator, Router, String, String) {
    let state = tempfile::tempdir().unwrap();
    let events = EventStore::new(state.path(), 100, true).unwrap();
    let operator = events.enroll("Operator", Role::Operator, true).unwrap();
    let staff = events.enroll("Staff", Role::Staff, false).unwrap();
    let coordinator = Coordinator::start(events, 7);
    let mut config = default_config(Some(state.path()));
    config["radio"]["simulator"]["enable"] = json!(true);
    let app = router(AppState {
        coordinator: coordinator.clone(),
        coordinator_id: "event".into(),
        radio_status: Arc::new(Mutex::new("disabled".into())),
        adapter: "disabled".into(),
        legacy: MishStore::new(state.path(), config).unwrap(),
        legacy_slots: Arc::new(tokio::sync::Semaphore::new(4)),
    });
    (state, coordinator, app, operator.token, staff.token)
}
#[tokio::test]
async fn staff_exchange_ack_and_published_projection_survive_restart() {
    let (state, coordinator, app, operator, staff) = setup();
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/messages",
            Some(&operator),
            json!({"text":"Private operator conversation"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/messages",
            Some(&staff),
            json!({"text":"Private staff reply"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (status,alert) = request(&app,"POST","/api/alerts",Some(&staff),json!({"text":"Private urgent alert","responder":"Supervisor","expires":mish::appliance::now()+3600.0,"kind":"urgent"})).await;
    assert_eq!(status, StatusCode::CREATED);
    let path = format!("/api/alerts/{}/ack", alert["id"].as_str().unwrap());
    assert_eq!(
        request(&app, "POST", &path, Some(&staff), json!({}))
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/announcements",
            Some(&operator),
            json!({"text":"Water station open"})
        )
        .await
        .0,
        StatusCode::CREATED
    );
    let (_, public) = request(&app, "GET", "/public/announcements", None, json!({})).await;
    assert!(!public.to_string().contains("Private"));
    let fields = public["announcements"][0].as_object().unwrap();
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        vec!["id", "published", "text"]
    );
    drop(app);
    drop(coordinator);
    let restarted = EventStore::new(state.path(), 100, true).unwrap();
    assert!(restarted.authenticate(&staff).unwrap().is_some());
    let recovered = restarted
        .record(alert["id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(recovered.acknowledgements.len(), 1);
    assert_eq!(restarted.records().unwrap().len(), 3);
}
#[tokio::test]
async fn private_reads_and_operator_writes_require_the_correct_credential() {
    let (_state, _coordinator, app, _operator, staff) = setup();
    for route in ["/api/messages", "/api/status", "/waypoints", "/geopacks"] {
        assert_eq!(
            request(&app, "GET", route, None, json!({})).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            request(&app, "GET", route, Some("invalid"), json!({}))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    for (method, path, body) in [
        (
            "POST",
            "/api/staff",
            json!({"name":"Injected operator","role":"operator"}),
        ),
        ("POST", "/api/announcements", json!({"text":"Unapproved"})),
        ("DELETE", "/api/staff/someone", json!({})),
        ("GET", "/waypoints", json!({})),
    ] {
        assert_eq!(
            request(&app, method, path, Some(&staff), body).await.0,
            StatusCode::FORBIDDEN
        );
    }
}
#[tokio::test]
async fn malformed_oversized_traversal_and_expired_commands_are_rejected() {
    let (_state, _coordinator, app, operator, _staff) = setup();
    for body in [
        json!([]),
        json!({"text":123}),
        json!({"text":""}),
        json!({"text":"x".repeat(161)}),
        json!({"text":"valid","unexpected":true}),
    ] {
        assert_eq!(
            request(&app, "POST", "/api/messages", Some(&operator), body)
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/messages",
            Some(&operator),
            json!({"text":"x".repeat(17000)})
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    for kind in ["message", "not-an-alert"] {
        assert_eq!(request(&app,"POST","/api/alerts",Some(&operator),json!({"text":"text","kind":kind,"expires":mish::appliance::now()+3600.0,"responder":"Staff"})).await.0,StatusCode::BAD_REQUEST);
    }
    assert_eq!(
        request(
            &app,
            "POST",
            "/api/alerts",
            Some(&operator),
            json!({"text":"text","expires":0,"responder":"Staff"})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        request(
            &app,
            "POST",
            "/waypoints/simulated",
            Some(&operator),
            json!({"id":"../escape","lat":0,"lon":0})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/messages")
                .header("authorization", format!("Bearer {operator}"))
                .header("content-type", "application/json")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
