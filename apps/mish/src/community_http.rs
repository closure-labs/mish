//! Cookie-authenticated attendee surface; bearer credentials remain staff-only.
use crate::{
    community::{Action, Credentials, Manage},
    server::{ApiError, AppState},
};
use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
fn credentials(headers: &HeaderMap) -> Credentials {
    Credentials {
        guest: headers
            .get("cookie")
            .and_then(|h| h.to_str().ok())
            .and_then(|cookies| {
                cookies.split(';').find_map(|c| {
                    c.trim()
                        .strip_prefix("__Host-mish-guest=")
                        .map(str::to_owned)
                })
            }),
        staff: headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .map(str::to_owned),
    }
}
fn same_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if host.is_empty() || origin != format!("https://{host}") {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "same-origin HTTPS required".into(),
        ));
    }
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Enrollment {
    nickname: String,
}
async fn enroll(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(v): Json<Enrollment>,
) -> Result<Response, ApiError> {
    same_origin(&h)?;
    let mut value = s
        .coordinator
        .community(Credentials::default(), Action::Enroll(v.nickname))
        .await?;
    let token = value.as_object_mut().unwrap().remove("token").unwrap();
    let cookie = format!(
        "__Host-mish-guest={}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=86400",
        token.as_str().unwrap()
    );
    Ok((StatusCode::CREATED, [("set-cookie", cookie)], Json(value)).into_response())
}
async fn session(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Session)
            .await?,
    ))
}
async fn rooms(
    State(s): State<AppState>,
    h: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Rooms)
            .await?,
    ))
}
async fn join(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    same_origin(&h)?;
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Join(id))
            .await?,
    ))
}
async fn leave(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    same_origin(&h)?;
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Leave(id))
            .await?,
    ))
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: i64,
    #[serde(default)]
    moderation_after: i64,
}
async fn messages(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Query(q): Query<Cursor>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut value = s
        .coordinator
        .community(
            credentials(&h),
            Action::Messages(id, q.after, q.moderation_after),
        )
        .await?;
    value["connection"] = json!(
        s.radio_status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "unavailable".into())
    );
    Ok(Json(value))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Message {
    key: String,
    text: String,
}
async fn post_message(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(id): Path<String>,
    Json(v): Json<Message>,
) -> Result<Json<serde_json::Value>, ApiError> {
    same_origin(&h)?;
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Post(id, v.key, v.text))
            .await?,
    ))
}
async fn manage(
    State(s): State<AppState>,
    h: HeaderMap,
    Json(v): Json<Manage>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Staff bearer authorization is not ambient cookie authority.
    Ok(Json(
        s.coordinator
            .community(credentials(&h), Action::Manage(v))
            .await?,
    ))
}
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/community/session", get(session).post(enroll))
        .route("/api/community/rooms", get(rooms))
        .route(
            "/api/community/rooms/{id}/membership",
            post(join).delete(leave),
        )
        .route(
            "/api/community/rooms/{id}/messages",
            get(messages).post(post_message),
        )
        .route("/api/community/manage", post(manage))
}
