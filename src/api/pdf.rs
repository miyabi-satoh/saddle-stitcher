//! PDF の中綴じ製本レイアウト変換 API。

use axum::extract::multipart::MultipartError;
use axum::extract::{Multipart, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::{AppError, ErrorResponse};
use crate::pdf::saddle_stitch::{Direction, saddle_stitch};
use crate::state::AppState;

/// OpenAPI ドキュメント用のリクエストボディ定義。
/// 実際のリクエストは `axum::extract::Multipart` で受け取るため、このスキーマは
/// ドキュメント生成のためだけに存在する (ハンドラの引数には使わない)。
#[derive(ToSchema)]
#[allow(dead_code)]
struct ConvertForm {
    #[schema(content_media_type = "application/pdf")]
    file: Vec<u8>,
    /// `"left"` (左開き) または `"right"` (右開き)
    direction: String,
}

#[utoipa::path(
    post,
    path = "/saddle-stitch",
    request_body(content = inline(ConvertForm), content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "変換済みの中綴じ製本レイアウトPDF", body = Vec<u8>, content_type = "application/pdf"),
        (status = 400, description = "リクエストが不正 (fileまたはdirectionが不足・不正)", body = ErrorResponse),
        (status = 422, description = "PDFとして処理できなかった (壊れている・パスワード付き等)", body = ErrorResponse),
    )
)]
async fn convert(
    State(_state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name: Option<String> = None;
    let mut direction: Option<Direction> = None;

    while let Some(field) = multipart.next_field().await.map_err(multipart_error)? {
        match field.name() {
            Some("file") => {
                file_name = field.file_name().map(str::to_string);
                file_bytes = Some(field.bytes().await.map_err(multipart_error)?.to_vec());
            }
            Some("direction") => {
                let text = field.text().await.map_err(multipart_error)?;
                direction = Some(parse_direction(&text)?);
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes.ok_or_else(|| AppError::BadRequest {
        message: "fileフィールドが必要です".to_string(),
    })?;
    let direction = direction.ok_or_else(|| AppError::BadRequest {
        message: "directionフィールドが必要です".to_string(),
    })?;

    let output = tokio::task::spawn_blocking(move || saddle_stitch(&file_bytes, direction))
        .await
        .map_err(|err| AppError::PdfProcessing {
            message: format!("処理タスクの実行に失敗しました: {err}"),
        })?
        .map_err(|err| AppError::PdfProcessing {
            message: err.to_string(),
        })?;

    let download_name = format!(
        "(製本版){}",
        file_name.unwrap_or_else(|| "output.pdf".to_string())
    );

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                content_disposition(&download_name),
            ),
        ],
        output,
    )
        .into_response())
}

fn parse_direction(text: &str) -> Result<Direction, AppError> {
    match text {
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        other => Err(AppError::BadRequest {
            message: format!(
                "directionは left か right を指定してください (受け取った値: \"{other}\")"
            ),
        }),
    }
}

fn multipart_error(err: MultipartError) -> AppError {
    AppError::InvalidMultipart {
        status: err.status(),
        message: err.body_text(),
    }
}

/// RFC 5987 に従い `filename*=UTF-8''<percent-encoded>` を付与する。
/// 併記する `filename=` は非ASCII文字を単純に落としたフォールバック
/// (対応していない古いクライアント向け。実害はブラウザでのダウンロード時のみ)。
const RFC5987_ATTR_CHAR: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'%')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'{')
    .add(b'}');

fn content_disposition(filename: &str) -> String {
    // `filename="..."` は quoted-string なので、`"` や `\` が残っていると壊れる
    // (エスケープする代わりに、フォールバック用途と割り切って単純に取り除く)。
    let ascii_fallback: String = filename
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control() && *c != '"' && *c != '\\')
        .collect();
    let ascii_fallback = if ascii_fallback.is_empty() {
        "output.pdf".to_string()
    } else {
        ascii_fallback
    };
    let encoded = utf8_percent_encode(filename, RFC5987_ATTR_CHAR);
    format!("attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{encoded}")
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(convert))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_disposition_ascii_fallback_strips_quote_and_backslash() {
        // `"` や `\` を含むファイル名がそのまま filename="..." に入ると
        // quoted-string として壊れてしまうため、取り除かれていることを確認する。
        let header = content_disposition("a\"b\\c.pdf");

        let part = header
            .split(';')
            .find(|part| part.trim_start().starts_with("filename=\""))
            .expect("should contain filename=\"...\"");
        let value = part
            .trim_start()
            .trim_start_matches("filename=\"")
            .trim_end_matches('"');
        assert!(!value.contains('"'), "{value}");
        assert!(!value.contains('\\'), "{value}");
    }

    #[test]
    fn content_disposition_includes_rfc5987_encoded_filename() {
        let header = content_disposition("(製本版)テスト.pdf");
        assert!(header.contains("filename*=UTF-8''"));
    }
}
