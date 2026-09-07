use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::{AppError, ErrorResponse};
use crate::state::AppState;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

// `path` はこのルーター自身から見た相対パス。`/api/v1` プレフィックスは `api::router()` 側で
// `OpenApiRouter::nest` するときに (axum のルーティングと OpenAPI ドキュメントの両方に)
// 自動的に付与される。
#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, description = "サーバーが正常に稼働している", body = HealthResponse),
        (status = 503, description = "DB に到達できない", body = ErrorResponse),
    )
)]
async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    // DB に実際に到達できるかまで確認する (単に起動しているだけでなく)。
    // `sqlx::query!` 系のコンパイル時チェックマクロは使わず実行時クエリにしている
    // (DATABASE_URL や .sqlx/ オフラインキャッシュを用意しなくても動くようにするため)。
    sqlx::query_scalar::<_, i64>("SELECT 1")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    }))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(health))
}
