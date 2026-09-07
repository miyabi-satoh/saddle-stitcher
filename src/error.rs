//! API 全体で共通のエラー型とレスポンス形式。

use axum::Json;
use axum::extract::rejection::{JsonRejection, PathRejection};
use axum::extract::{FromRequest, FromRequestParts, Path, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use utoipa::ToSchema;

/// API 全体で共通のエラー型。バリアントは実際にそれを生成する箇所ができた時点で追加する
/// (投機的に増やさない)。認証・バリデーション等のバリアントはドメインを実装する際に追加する。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("database error")]
    Database(#[from] sqlx::Error),
    /// リクエストボディが不正 (JSON として解釈できない・必須フィールド欠落・
    /// `Content-Type` が `application/json` でない等)。`axum::Json` の rejection を
    /// そのまま返すと共通 envelope にならないため、`AppJson` 経由でここに変換する。
    #[error("{message}")]
    InvalidJson { status: StatusCode, message: String },
    /// パスパラメータが不正 (`/things/{id}` の `id` が数値として解釈できない等)。
    /// `axum::extract::Path` の rejection をそのまま返すと `text/plain` になり共通 envelope
    /// にならないため、`AppPath` 経由でここに変換する。
    #[error("{message}")]
    InvalidPath { status: StatusCode, message: String },
    /// multipart フォームの読み取り自体に失敗 (不正な境界・サイズ超過等)。
    #[error("{message}")]
    InvalidMultipart { status: StatusCode, message: String },
    /// multipart は読めたがフィールドの内容が不正 (ファイル未指定・`direction` の値が
    /// `left`/`right` 以外等)。
    #[error("{message}")]
    BadRequest { message: String },
    /// アップロードされたファイルを PDF として処理できなかった
    /// (壊れている・パスワード付きで復号できない・ページが無い等)。
    #[error("{message}")]
    PdfProcessing { message: String },
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            Self::Database(_) => (StatusCode::SERVICE_UNAVAILABLE, "database_unavailable"),
            Self::InvalidJson { status, .. } => (*status, "invalid_request_body"),
            Self::InvalidPath { status, .. } => (*status, "invalid_path"),
            Self::InvalidMultipart { status, .. } => (*status, "invalid_multipart"),
            Self::BadRequest { .. } => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::PdfProcessing { .. } => {
                (StatusCode::UNPROCESSABLE_ENTITY, "pdf_processing_failed")
            }
        }
    }
}

/// レスポンスボディの共通 envelope。`code` が機械可読な契約 (frontend はこちらで表示文言を引く)、
/// `message` はローカライズしないデバッグ用の説明文。
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Serialize, ToSchema)]
struct ErrorBody {
    code: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        if status.is_server_error() {
            // Debug でログに出す: `Database` バリアントの元の sqlx::Error まで残すため。
            // レスポンスボディ側 (Display) は DB 内部の詳細を漏らさない定型文のまま。
            tracing::error!(error = ?self, "API エラー");
        }
        let body = ErrorResponse {
            error: ErrorBody {
                code,
                message: self.to_string(),
            },
        };
        (status, Json(body)).into_response()
    }
}

/// `axum::Json` の代わりに使う JSON body エクストラクタ。
/// 抽出失敗 (不正な JSON・必須フィールド欠落・`Content-Type` 不一致) を、素の
/// axum レスポンスではなく `AppError` (共通 envelope) に変換する。
pub struct AppJson<T>(pub T);

impl<S, T> FromRequest<S> for AppJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(AppJson(value)),
            Err(rejection) => Err(json_rejection_to_app_error(rejection)),
        }
    }
}

fn json_rejection_to_app_error(rejection: JsonRejection) -> AppError {
    AppError::InvalidJson {
        status: rejection.status(),
        message: rejection.body_text(),
    }
}

/// `axum::extract::Path` の代わりに使うパスパラメータエクストラクタ。
/// 抽出失敗 (数値であるべき id が数値でない等) を、素の axum レスポンス
/// (`text/plain`) ではなく `AppError` (共通 envelope) に変換する。
pub struct AppPath<T>(pub T);

impl<S, T> FromRequestParts<S> for AppPath<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Send,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(value)) => Ok(AppPath(value)),
            Err(rejection) => Err(path_rejection_to_app_error(rejection)),
        }
    }
}

fn path_rejection_to_app_error(rejection: PathRejection) -> AppError {
    AppError::InvalidPath {
        status: rejection.status(),
        message: rejection.body_text(),
    }
}
