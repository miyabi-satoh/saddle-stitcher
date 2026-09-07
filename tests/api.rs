//! `/api/v1/*` を HTTP レベルで通しで検証する統合テスト。

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use saddle_stitcher::build_app;
use serde_json::Value;
use sqlx::SqlitePool;
use tower::ServiceExt;

fn test_app(pool: SqlitePool) -> Router {
    build_app(pool)
}

fn get(uri: &str) -> Request<Body> {
    Request::get(uri)
        .body(Body::empty())
        .expect("failed to build request")
}

async fn json_body(response: Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("failed to read body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("response body should be JSON")
}

#[sqlx::test]
async fn health_returns_ok_with_version(pool: SqlitePool) {
    let app = test_app(pool);

    let response = app.oneshot(get("/api/v1/health")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[sqlx::test]
async fn health_returns_503_when_db_unavailable(pool: SqlitePool) {
    let app = test_app(pool.clone());
    pool.close().await;

    let response = app.oneshot(get("/api/v1/health")).await.unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "database_unavailable"
    );
}

#[sqlx::test]
async fn unknown_api_path_returns_json_404(pool: SqlitePool) {
    let app = test_app(pool);

    let response = app.oneshot(get("/api/v1/nope")).await.unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/json")
    );
    assert_eq!(json_body(response).await["error"]["code"], "not_found");
}

#[sqlx::test]
async fn unknown_non_api_path_falls_back_to_spa(pool: SqlitePool) {
    let app = test_app(pool);

    let response = app.oneshot(get("/nope")).await.unwrap();

    // frontend/build に index.html がある前提 (無ければ 404)。
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("text/html")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok()),
        Some("no-cache")
    );
}
