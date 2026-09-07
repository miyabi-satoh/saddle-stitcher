pub mod api;
pub mod cli;
pub mod config;
pub mod db;
pub mod error;
pub mod logging;
pub mod state;
pub mod static_files;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// アプリケーション全体の `Router` を組み立てる。
/// `main.rs` と統合テストの両方から共通で呼べるように公開している。
pub fn build_app(pool: SqlitePool) -> Router {
    let state = AppState { pool };

    let (api_router, _openapi) = api::router().split_for_parts();
    api_router
        .fallback(static_files::handler)
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .with_state(state)
}
