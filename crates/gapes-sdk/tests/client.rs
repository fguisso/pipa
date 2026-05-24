//! Lightweight smoke tests for `gapes-sdk::Client`. We spin up a small axum
//! app on a random port with three canned routes and verify:
//!   1. Method + headers + body shape are what the SDK actually sends.
//!   2. The JSON the server returns parses into the SDK's models.
//!
//! Goal is "the SDK survives an upgrade without us noticing", NOT exhaustive
//! coverage of every endpoint.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::{Json, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete as delete_route, post};
use gapes_sdk::{Client, DeployParams};
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[derive(Default, Clone)]
struct Spy {
    mint_body: Arc<Mutex<Option<Value>>>,
    deploy_seen_uuid: Arc<Mutex<Option<String>>>,
    deploy_bearer: Arc<Mutex<Option<String>>>,
    deploy_content_type: Arc<Mutex<Option<String>>>,
    deploy_archive_bytes: Arc<Mutex<Option<Vec<u8>>>>,
    delete_stepup: Arc<Mutex<Option<String>>>,
    delete_bearer: Arc<Mutex<Option<String>>>,
    delete_path_uuid: Arc<Mutex<Option<String>>>,
}

#[derive(Clone)]
struct AppState {
    spy: Spy,
}

async fn spawn() -> (Spy, String) {
    let spy = Spy::default();
    let state = AppState { spy: spy.clone() };

    let app = Router::new()
        .route("/api/auth/mint", post(mint))
        .route("/api/pages", post(deploy))
        .route("/api/pages/:uuid", delete_route(delete_one).get(get_one))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr: SocketAddr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (spy, format!("http://{addr}"))
}

async fn mint(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Json<Value> {
    *state.spy.mint_body.lock().unwrap() = Some(body);
    Json(json!({
        "access": "acc_xyz",
        "refresh": "ref_new_xyz",
        "expires": 300,
    }))
}

async fn deploy(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: axum::extract::Multipart,
) -> (StatusCode, Json<Value>) {
    *state.spy.deploy_bearer.lock().unwrap() = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    *state.spy.deploy_content_type.lock().unwrap() = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "archive" {
            let bytes = field.bytes().await.unwrap_or_default();
            *state.spy.deploy_archive_bytes.lock().unwrap() = Some(bytes.to_vec());
        } else {
            let _ = field.bytes().await;
        }
    }

    let uuid = "01HXYZTEST00000000DEPLOY";
    *state.spy.deploy_seen_uuid.lock().unwrap() = Some(uuid.into());
    (
        StatusCode::OK,
        Json(json!({
            "uuid": uuid,
            "url": format!("http://127.0.0.1/p/{uuid}"),
            "size_bytes": 12u64,
            "file_count": 1u64,
            "mode": "spa",
            "visibility": "public",
        })),
    )
}

async fn delete_one(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    headers: HeaderMap,
) -> StatusCode {
    *state.spy.delete_path_uuid.lock().unwrap() = Some(uuid);
    *state.spy.delete_bearer.lock().unwrap() = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    *state.spy.delete_stepup.lock().unwrap() = headers
        .get("x-stepup-code")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    StatusCode::NO_CONTENT
}

async fn get_one(
    State(_state): State<AppState>,
    Path(uuid): Path<String>,
) -> Json<Value> {
    Json(json!({
        "uuid": uuid,
        "name": null,
        "mode": "spa",
        "visibility": "public",
        "owner_kind": "local",
        "owner_id": "local",
        "size_bytes": 12u64,
        "file_count": 1u64,
        "created_at": 0i64,
        "updated_at": 0i64,
    }))
}

#[tokio::test(flavor = "multi_thread")]
async fn mint_sends_refresh_and_scope_and_parses_response() {
    let (spy, base) = spawn().await;
    let mut client = Client::new(&base, Some("ref_initial".into())).expect("client");

    let resp = client
        .mint_access("deploy:new", 60)
        .await
        .expect("mint_access");
    assert_eq!(resp.access, "acc_xyz");
    assert_eq!(resp.refresh, "ref_new_xyz");
    assert_eq!(resp.expires, 300);

    // The stored refresh should have been rotated.
    assert_eq!(client.refresh(), Some("ref_new_xyz"));

    // Spy: the request body shape is { refresh, scope, ttl_sec }.
    let body = spy.mint_body.lock().unwrap().clone().expect("body captured");
    assert_eq!(body["refresh"].as_str(), Some("ref_initial"));
    assert_eq!(body["scope"].as_str(), Some("deploy:new"));
    assert_eq!(body["ttl_sec"].as_u64(), Some(60));
}

#[tokio::test(flavor = "multi_thread")]
async fn deploy_archive_sends_multipart_with_bearer() {
    let (spy, base) = spawn().await;
    let client = Client::new(&base, None).expect("client");

    let zip_bytes = b"PK\x03\x04fake-zip-bytes".to_vec();
    let resp = client
        .deploy_archive(
            "ACCESS_TOK",
            zip_bytes.clone(),
            DeployParams {
                visibility: Some("public".into()),
                ..Default::default()
            },
        )
        .await
        .expect("deploy_archive");
    assert_eq!(resp.uuid, "01HXYZTEST00000000DEPLOY");
    assert_eq!(resp.visibility, "public");

    let bearer = spy.deploy_bearer.lock().unwrap().clone();
    assert_eq!(bearer.as_deref(), Some("Bearer ACCESS_TOK"));

    let content_type = spy.deploy_content_type.lock().unwrap().clone();
    assert!(
        content_type
            .as_deref()
            .unwrap_or("")
            .starts_with("multipart/form-data"),
        "content-type should be multipart, got {content_type:?}"
    );

    let archive = spy.deploy_archive_bytes.lock().unwrap().clone();
    assert_eq!(archive.as_deref(), Some(zip_bytes.as_slice()));
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_page_sends_stepup_header_and_bearer() {
    let (spy, base) = spawn().await;
    let client = Client::new(&base, None).expect("client");

    client
        .delete_page("ACC", "01HXYZ_PAGE", "9K3X-MN7P")
        .await
        .expect("delete");

    assert_eq!(
        spy.delete_path_uuid.lock().unwrap().as_deref(),
        Some("01HXYZ_PAGE"),
    );
    assert_eq!(
        spy.delete_bearer.lock().unwrap().as_deref(),
        Some("Bearer ACC"),
    );
    assert_eq!(
        spy.delete_stepup.lock().unwrap().as_deref(),
        Some("9K3X-MN7P"),
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn get_page_parses_page_view_response() {
    let (_spy, base) = spawn().await;
    let client = Client::new(&base, None).expect("client");

    let pv = client.get_page("ACC", "01HXYZ_PAGE").await.expect("get_page");
    assert_eq!(pv.uuid, "01HXYZ_PAGE");
    assert_eq!(pv.mode, "spa");
    assert_eq!(pv.visibility, "public");
    assert_eq!(pv.size_bytes, 12);
    assert_eq!(pv.file_count, 1);
}
