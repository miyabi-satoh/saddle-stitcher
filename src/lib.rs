pub mod api;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod logging;
pub mod pdf;
pub mod state;
pub mod static_files;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use sqlx::SqlitePool;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// アプリケーション全体の `Router` を組み立てる。
/// `main.rs` と統合テストの両方から共通で呼べるように公開している。
///
/// `max_upload_bytes` は axum のデフォルトボディサイズ上限 (2MB) を上書きする値。
/// PDFアップロードを受け付けるエンドポイントがあるため、既定より大きい値を渡す想定
/// (`config.toml` の `[server] max_upload_bytes` から渡される。詳細は `config::ServerConfig`)。
pub fn build_app(pool: SqlitePool, max_upload_bytes: usize) -> Router {
    let state = AppState { pool };

    let (api_router, _openapi) = api::router().split_for_parts();
    api_router
        .fallback(static_files::handler)
        .layer(DefaultBodyLimit::max(max_upload_bytes))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .with_state(state)
}
