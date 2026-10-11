//! Typed HTTP boundary for scoped staff coordination and community enrollment.
use crate::{InputError, coordinator::Coordinator, domain::*};
use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use serde::Deserialize;
use serde_json::json;

#[derive(Clone)]
pub struct AppState {
    pub coordinator: Coordinator,
    pub coordinator_id: String,
    pub radio_status: std::sync::Arc<std::sync::Mutex<String>>,
    pub adapter: String,
    pub legacy: crate::storage::MishStore,
    pub legacy_slots: std::sync::Arc<tokio::sync::Semaphore>,
}
pub struct ApiError(StatusCode, String);
impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        if error.downcast_ref::<InputError>().is_some() {
            Self(StatusCode::BAD_REQUEST, error.to_string())
        } else {
            eprintln!("mish request failed: {error:#}");
            Self(
                StatusCode::SERVICE_UNAVAILABLE,
                "coordinator unavailable".into(),
            )
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
async fn protect(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let token = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    match token {
        Some(token) => match state.coordinator.authenticate(token).await {
            Ok(Some(actor)) => {
                request.extensions_mut().insert(actor);
                next.run(request).await
            }
            Ok(None) => ApiError(StatusCode::UNAUTHORIZED, "invalid staff credential".into())
                .into_response(),
            Err(error) => ApiError::from(error).into_response(),
        },
        None => {
            ApiError(StatusCode::UNAUTHORIZED, "staff credential required".into()).into_response()
        }
    }
}
async fn headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("cache-control", "no-store".parse().unwrap());
    headers.insert("x-content-type-options", "nosniff".parse().unwrap());
    headers.insert("content-security-policy", "default-src 'self'; img-src 'self' data:; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; frame-ancestors 'none'".parse().unwrap());
    response
}
fn operator(actor: &StaffIdentity) -> Result<(), ApiError> {
    if actor.role != Role::Operator {
        return Err(ApiError(StatusCode::FORBIDDEN, "operator required".into()));
    }
    Ok(())
}
fn parse<T: serde::de::DeserializeOwned>(
    body: Result<Json<T>, axum::extract::rejection::JsonRejection>,
) -> Result<T, ApiError> {
    body.map(|Json(value)| value).map_err(|error| {
        ApiError(
            if error.status() == StatusCode::PAYLOAD_TOO_LARGE {
                error.status()
            } else {
                StatusCode::BAD_REQUEST
            },
            "invalid JSON request".into(),
        )
    })
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageRequest {
    text: MessageText,
    #[serde(default = "default_channel")]
    channel: RecordId,
}
fn default_channel() -> RecordId {
    RecordId::new("staff").unwrap()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AlertRequest {
    text: MessageText,
    #[serde(default = "default_channel")]
    channel: RecordId,
    responder: StaffName,
    expires: f64,
    #[serde(default = "alert_kind")]
    kind: MessageKind,
}
fn alert_kind() -> MessageKind {
    MessageKind::Alert
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnrollmentRequest {
    name: StaffName,
    #[serde(default = "staff_role")]
    role: Role,
}
fn staff_role() -> Role {
    Role::Staff
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnouncementRequest {
    text: AnnouncementText,
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({"ok":true,"coordinator":state.coordinator_id}))
}
async fn staff_page() -> Html<&'static str> {
    Html(include_str!("../static/staff.html"))
}
async fn channel_selection_script() -> impl IntoResponse {
    (
        [("content-type", "application/javascript; charset=utf-8")],
        include_str!("../static/channel-selection.js"),
    )
}
async fn public_page() -> Html<&'static str> {
    Html(include_str!("../static/public.html"))
}
async fn public(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"announcements":state.coordinator.public().await?}),
    ))
}
async fn status(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(
        json!({"radio":state.radio_status.lock().map(|v| v.clone()).unwrap_or_else(|_| "unavailable".into()),"adapter":state.adapter}),
    )
}
async fn messages(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    Query(query): Query<MessageQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let records = match query.channel {
        Some(id) => {
            state
                .coordinator
                .records_for_channel(actor, id.as_str())
                .await?
        }
        None => state.coordinator.records_for(actor).await?,
    };
    Ok(Json(json!({"messages":records})))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MessageQuery {
    channel: Option<RecordId>,
}
async fn message(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    body: Result<Json<MessageRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let value = parse(body)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .coordinator
                .create_as(
                    actor.clone(),
                    NewRecord::message(value.text, &actor.id)?
                        .on_channel(value.channel.as_str())?,
                )
                .await?,
        ),
    ))
}
async fn alert(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    body: Result<Json<AlertRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let value = parse(body)?;
    if value.kind == MessageKind::Message {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "alert kind required".into(),
        ));
    }
    let record = NewRecord::alert(
        value.text,
        &actor.id,
        value.responder,
        value.expires,
        value.kind == MessageKind::Urgent,
        crate::appliance::now(),
    )?
    .on_channel(value.channel.as_str())?;
    Ok((
        StatusCode::CREATED,
        Json(state.coordinator.create_as(actor, record).await?),
    ))
}
async fn acknowledge(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    Path(id): Path<String>,
) -> Result<Json<Acknowledgement>, ApiError> {
    let id = RecordId::new(id)?;
    Ok(Json(
        state
            .coordinator
            .acknowledge(
                id.as_str(),
                &actor.id,
                AcknowledgementSource::Browser,
                None,
                None,
            )
            .await?,
    ))
}
async fn enroll(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    body: Result<Json<EnrollmentRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    operator(&actor)?;
    let value = parse(body)?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .coordinator
                .enroll(value.name.as_str(), value.role, false)
                .await?,
        ),
    ))
}
async fn revoke(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    operator(&actor)?;
    let id = RecordId::new(id)?;
    state.coordinator.revoke(id.as_str()).await?;
    Ok(Json(json!({"revoked":id})))
}
async fn publish(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    body: Result<Json<AnnouncementRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    operator(&actor)?;
    let value = parse(body)?;
    Ok((
        StatusCode::CREATED,
        Json(state.coordinator.publish(value.text.as_str()).await?),
    ))
}

async fn channel_catalog(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"channels":state.coordinator.channels(Some(actor)).await?}),
    ))
}
async fn community_channels(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    Ok(Json(
        json!({"channels":state.coordinator.channels(None).await?}),
    ))
}
async fn community_enrollment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::channel_store::EnrollmentExport>, ApiError> {
    let id = RecordId::new(id)?;
    Ok(Json(
        state
            .coordinator
            .channel_enrollment(id.as_str(), None)
            .await?,
    ))
}
async fn channel_enrollment(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    Path(id): Path<String>,
) -> Result<Json<crate::channel_store::EnrollmentExport>, ApiError> {
    let id = RecordId::new(id)?;
    Ok(Json(
        state
            .coordinator
            .channel_enrollment(id.as_str(), Some(actor))
            .await?,
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChannelAccessRequest {
    staff_id: RecordId,
    grant: bool,
}
async fn channel_access(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    Path(id): Path<String>,
    body: Result<Json<ChannelAccessRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    operator(&actor)?;
    let id = RecordId::new(id)?;
    let value = parse(body)?;
    state
        .coordinator
        .channel_access(&actor.id, value.staff_id.as_str(), id.as_str(), value.grant)
        .await?;
    Ok(Json(json!({"updated":true})))
}
async fn legacy(
    State(state): State<AppState>,
    axum::Extension(actor): axum::Extension<StaffIdentity>,
    request: Request<Body>,
) -> Result<Response, ApiError> {
    operator(&actor)?;
    let path = request.uri().path().to_owned();
    let method = request.method().clone();
    let body = if method == axum::http::Method::POST {
        let bytes = axum::body::to_bytes(request.into_body(), 16384)
            .await
            .map_err(|_| ApiError(StatusCode::PAYLOAD_TOO_LARGE, "request too large".into()))?;
        serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid JSON request".into()))?
    } else {
        json!({})
    };
    if !body.is_object() {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "JSON object required".into(),
        ));
    }
    let permit = state
        .legacy_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError(
                StatusCode::SERVICE_UNAVAILABLE,
                "legacy storage busy".into(),
            )
        })?;
    let result =
        tokio::task::spawn_blocking(move || -> anyhow::Result<(StatusCode, serde_json::Value)> {
            let _permit = permit;
            if method == axum::http::Method::GET {
                return Ok((
                    StatusCode::OK,
                    if path == "/waypoints" {
                        json!({"waypoints":state.legacy.list_waypoints()?})
                    } else {
                        json!({"geopacks":state.legacy.list_geopacks()?})
                    },
                ));
            }
            let visibility = body
                .get("visibility")
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| crate::invalid("visibility must be text"))
                })
                .transpose()?
                .unwrap_or("private");
            let id = body
                .get("id")
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| crate::invalid("identifier must be text"))
                })
                .transpose()?;
            let (item, created) = if path == "/waypoints/simulated" {
                if state.legacy.config["radio"]["simulator"]["enable"] != true {
                    return Err(crate::invalid("simulation disabled"));
                }
                let lat = body
                    .get("lat")
                    .unwrap_or(&body["position"]["lat"])
                    .as_f64()
                    .ok_or_else(|| crate::invalid("latitude required"))?;
                let lon = body
                    .get("lon")
                    .unwrap_or(&body["position"]["lon"])
                    .as_f64()
                    .ok_or_else(|| crate::invalid("longitude required"))?;
                let label = body
                    .get("label")
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| crate::invalid("label must be text"))
                    })
                    .transpose()?;
                let text = body
                    .get("body")
                    .map(|v| {
                        v.as_str()
                            .ok_or_else(|| crate::invalid("body must be text"))
                    })
                    .transpose()?;
                state.legacy.add_simulated_waypoint(
                    lat,
                    lon,
                    label,
                    text,
                    visibility,
                    id,
                    body.get("payload").cloned().unwrap_or_else(|| json!({})),
                )?
            } else {
                let name = body["name"]
                    .as_str()
                    .ok_or_else(|| crate::invalid("name required"))?;
                let ids = body["waypointIds"]
                    .as_array()
                    .ok_or_else(|| crate::invalid("waypoint list required"))?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(String::from)
                            .ok_or_else(|| crate::invalid("waypoint identifier required"))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                state.legacy.create_geopack(name, ids, visibility, id)?
            };
            Ok((
                if created {
                    StatusCode::CREATED
                } else {
                    StatusCode::OK
                },
                json!({"item":item,"created":created}),
            ))
        })
        .await
        .map_err(|error| ApiError::from(anyhow::Error::from(error)))??;
    Ok((result.0, Json(result.1)).into_response())
}
pub fn router(state: AppState) -> Router {
    let private = Router::new()
        .route("/api/status", get(status))
        .route("/api/messages", get(messages).post(message))
        .route("/api/alerts", post(alert))
        .route("/api/alerts/{id}/ack", post(acknowledge))
        .route("/api/staff", post(enroll))
        .route("/api/staff/{id}", delete(revoke))
        .route("/api/announcements", post(publish))
        .route("/api/channels", get(channel_catalog))
        .route("/api/channels/{id}/enrollment", get(channel_enrollment))
        .route("/api/channels/{id}/access", post(channel_access))
        .route("/waypoints", get(legacy))
        .route("/waypoints/simulated", post(legacy))
        .route("/geopacks", get(legacy).post(legacy))
        .route_layer(middleware::from_fn_with_state(state.clone(), protect));
    Router::new()
        .route("/health", get(health))
        .route("/", get(staff_page))
        .route("/staff", get(staff_page))
        .route("/channel-selection.js", get(channel_selection_script))
        .route("/public", get(public_page))
        .route("/public/channels", get(community_channels))
        .route(
            "/public/channels/{id}/enrollment",
            get(community_enrollment),
        )
        .route("/public/announcements", get(public))
        .merge(private)
        .fallback(|| async { ApiError(StatusCode::NOT_FOUND, "route not found".into()) })
        .layer(DefaultBodyLimit::max(16384))
        .layer(middleware::from_fn(headers))
        .with_state(state)
}

pub async fn serve(host: &str, port: u16, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
        })
        .await?;
    Ok(())
}
