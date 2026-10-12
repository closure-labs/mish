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

#[tokio::test]
async fn guest_cookies_require_https_same_origin_and_never_authorize_staff() {
    let (_dir, coordinator, app, operator, _) = setup();
    coordinator
        .community(
            mish::community::Credentials {
                staff: Some(operator),
                guest: None,
            },
            mish::community::Action::Manage(mish::community::Manage::Configure {
                enabled: true,
                enroll_per_hour: 120,
                posts_per_minute: 30,
                queue_limit: 10,
            }),
        )
        .await
        .unwrap();
    let enroll = |origin: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/community/session")
            .header("host", "event.test")
            .header("origin", origin)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"nickname":"Guest"}"#))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(enroll("https://evil.test"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.clone()
            .oneshot(enroll("http://event.test"))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(enroll("https://event.test"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();
    for flag in ["Secure", "HttpOnly", "SameSite=Strict", "Path=/"] {
        assert!(cookie.contains(flag));
    }
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert!(body.get("token").is_none());
    let request = |path: &str| {
        Request::builder()
            .uri(path)
            .header("cookie", cookie.split(';').next().unwrap())
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(request("/api/community/session"))
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        app.clone()
            .oneshot(request("/api/messages"))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
}

/// Real-time HTTP soak: 50 independent sessions, 1 aggregate post/sec, 2-sec polling.
/// Run explicitly in native CI; shortened local runs are smoke checks only.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "one-hour qualification; run with --ignored --exact community_http_soak"]
async fn community_http_soak() {
    use std::time::{Duration, Instant};
    let (_dir, coordinator, app, operator, _) = setup();
    let op = mish::community::Credentials {
        staff: Some(operator),
        guest: None,
    };
    coordinator
        .community(
            op.clone(),
            mish::community::Action::Manage(mish::community::Manage::Configure {
                enabled: true,
                enroll_per_hour: 120,
                posts_per_minute: 30,
                queue_limit: 10,
            }),
        )
        .await
        .unwrap();
    let room = coordinator
        .community(
            op,
            mish::community::Action::Manage(mish::community::Manage::Create {
                name: mish::domain::Text::new("Load").unwrap(),
                public: true,
                channel: None,
            }),
        )
        .await
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let url = format!("http://{address}/api/community");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let mut cookies = Vec::new();
    for i in 0..50 {
        let response = client
            .post(format!("{url}/session"))
            .header("Origin", format!("https://{address}"))
            .json(&json!({"nickname":format!("Guest{i}")}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        client
            .post(format!("{url}/rooms/{room}/membership"))
            .header("Origin", format!("https://{address}"))
            .header("Cookie", &cookie)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        cookies.push(cookie);
    }
    let seconds: u64 = std::env::var("MISH_SOAK_SECONDS")
        .ok()
        .map(|s| s.parse().unwrap())
        .unwrap_or(3600);
    let start = Instant::now();
    let mut readers = Vec::new();
    for cookie in &cookies {
        let (client, url, room, cookie) =
            (client.clone(), url.clone(), room.clone(), cookie.clone());
        readers.push(tokio::spawn(async move {
            let mut cursor = 0;
            let mut seen = 0;
            loop {
                let page: Value = client
                    .get(format!("{url}/rooms/{room}/messages?after={cursor}"))
                    .header("Cookie", &cookie)
                    .send()
                    .await
                    .unwrap()
                    .error_for_status()
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                for entry in page["entries"].as_array().unwrap() {
                    let posted: f64 = entry["text"].as_str().unwrap().parse().unwrap();
                    assert!(
                        mish::appliance::now() - posted < 5.0,
                        "local update exceeded five seconds"
                    );
                    seen += 1;
                }
                cursor = page["cursor"].as_i64().unwrap();
                if seen == seconds {
                    return seen;
                }
                assert!(
                    start.elapsed() < Duration::from_secs(seconds + 10),
                    "accepted message missing"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }));
    }
    for i in 0..seconds {
        tokio::time::sleep_until(tokio::time::Instant::from_std(
            start + Duration::from_secs(i),
        ))
        .await;
        let response: Value = client
            .post(format!("{url}/rooms/{room}/messages"))
            .header("Origin", format!("https://{address}"))
            .header("Cookie", &cookies[i as usize % 50])
            .json(&json!({"key":format!("load-{i}"),"text":mish::appliance::now().to_string()}))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(response["accepted"], true);
    }
    for task in readers {
        assert_eq!(task.await.unwrap(), seconds);
    }
    server.abort();
    eprintln!(
        "50 HTTP sessions observed all {seconds} messages within five seconds; elapsed {:?}",
        start.elapsed()
    );
}
