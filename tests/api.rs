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

const TEST_MAX_UPLOAD_BYTES: usize = 200 * 1024 * 1024;

fn test_app(pool: SqlitePool) -> Router {
    build_app(pool, TEST_MAX_UPLOAD_BYTES)
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

/// A4 縦1ページだけの最小限の PDF を生成する (テスト用サンプル入力)。
fn sample_pdf_bytes() -> Vec<u8> {
    use lopdf::{Document, Object, Stream, dictionary};

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(lopdf::Dictionary::new(), b"BT ET".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => 1,
            "Kids" => vec![Object::Reference(page_id)],
        }),
    );
    let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog_id);

    let mut buffer = Vec::new();
    doc.save_to(&mut buffer).unwrap();
    buffer
}

/// `multipart/form-data` のリクエストボディを手組みする
/// (`direction` フィールドと `file` フィールドの2つだけを持つ、この API 専用の簡易ヘルパー)。
fn saddle_stitch_multipart_body(
    boundary: &str,
    direction: &str,
    file_bytes: &[u8],
    filename: &str,
) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"direction\"\r\n\r\n");
    body.extend_from_slice(direction.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/pdf\r\n\r\n");
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    body
}

fn multipart_request(uri: &str, boundary: &str, body: Vec<u8>) -> Request<Body> {
    Request::post(uri)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("failed to build request")
}

#[sqlx::test]
async fn saddle_stitch_converts_pdf_and_sets_content_disposition(pool: SqlitePool) {
    let app = test_app(pool);
    let boundary = "test-boundary";
    let body = saddle_stitch_multipart_body(boundary, "left", &sample_pdf_bytes(), "元原稿.pdf");

    let response = app
        .oneshot(multipart_request("/api/v1/saddle-stitch", boundary, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok()),
        Some("application/pdf")
    );
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    assert!(disposition.contains("filename*=UTF-8''"), "{disposition}");

    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let out_doc = lopdf::Document::load_mem(&bytes).expect("output should be a valid PDF");
    // 1ページ -> 4ページ分に空白パディング -> 2見開き
    assert_eq!(out_doc.get_pages().len(), 2);
}

#[sqlx::test]
async fn saddle_stitch_missing_file_returns_bad_request(pool: SqlitePool) {
    let app = test_app(pool);
    let boundary = "test-boundary";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Disposition: form-data; name=\"direction\"\r\n\r\nleft\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let response = app
        .oneshot(multipart_request("/api/v1/saddle-stitch", boundary, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"]["code"], "bad_request");
}

#[sqlx::test]
async fn saddle_stitch_invalid_direction_returns_bad_request(pool: SqlitePool) {
    let app = test_app(pool);
    let boundary = "test-boundary";
    let body = saddle_stitch_multipart_body(boundary, "up", &sample_pdf_bytes(), "a.pdf");

    let response = app
        .oneshot(multipart_request("/api/v1/saddle-stitch", boundary, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(response).await["error"]["code"], "bad_request");
}

#[sqlx::test]
async fn saddle_stitch_non_pdf_file_returns_unprocessable(pool: SqlitePool) {
    let app = test_app(pool);
    let boundary = "test-boundary";
    let body = saddle_stitch_multipart_body(boundary, "left", b"not a pdf", "a.pdf");

    let response = app
        .oneshot(multipart_request("/api/v1/saddle-stitch", boundary, body))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        json_body(response).await["error"]["code"],
        "pdf_processing_failed"
    );
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
