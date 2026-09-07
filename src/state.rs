use sqlx::SqlitePool;

/// axum ハンドラ間で共有するアプリケーション状態。
/// `SqlitePool` は内部で `Arc` を持つため、`Clone` しても実体は共有される。
#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}
