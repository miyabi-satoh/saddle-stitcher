//! `/api/v1` 配下の HTTP API と OpenAPI ドキュメント。

mod health;

use utoipa::openapi::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::error::{AppError, ErrorResponse};
use crate::state::AppState;

/// OpenAPI ドキュメントのベース (タイトル・スキーマ登録)。
/// `ErrorResponse` はどのハンドラの戻り値にも直接現れない (`AppError` 経由でレスポンスになる)
/// ため、明示的に登録しないと OpenAPI スキーマから漏れる。
#[derive(utoipa::OpenApi)]
#[openapi(info(title = "saddle-stitcher"), components(schemas(ErrorResponse)))]
struct ApiDoc;

/// `/api/v1` を nest した、state 未確定のルーター。
/// `Router<AppState>` はまだ `AppState` の実体 (DB 接続プール) を必要とせず組み立てられる
/// ため、`--openapi` のように DB に触れず OpenAPI ドキュメントだけ欲しい場合はここで止めてよい。
///
/// `fallback` を nest される側に設定しておくのが重要: axum の `nest` はネストするルーター自身が
/// fallback を持つ場合はそれを引き継ぐため、`/api/v1/*` 配下の未知のパスが SPA の `index.html`
/// にフォールバックしてしまうのを防げる。
pub fn router() -> OpenApiRouter<AppState> {
    let v1 = OpenApiRouter::new()
        .merge(health::router())
        .fallback(|| async { AppError::NotFound });

    OpenApiRouter::with_openapi(<ApiDoc as utoipa::OpenApi>::openapi()).nest("/api/v1", v1)
}

/// OpenAPI ドキュメントを組み立てる。DB には一切触れない。
pub fn openapi() -> OpenApi {
    let (_router, mut openapi) = router().split_for_parts();
    openapi.info.version = env!("CARGO_PKG_VERSION").to_string();
    // Cargo.toml に description/license を書いていないため、derive が生成した空文字列を
    // 空欄のまま出すよりは省いておく。
    openapi.info.description = None;
    openapi.info.license = None;
    openapi
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_doc_has_prefixed_paths_and_schemas() {
        let openapi = openapi();
        let json = openapi.to_json().unwrap();
        for needle in ["/api/v1/health", "HealthResponse", "ErrorResponse"] {
            assert!(json.contains(needle), "missing {needle}: {json}");
        }
        assert!(openapi.info.license.is_none());
        assert!(openapi.servers.is_none());
    }
}
