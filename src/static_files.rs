//! SvelteKit (SPA) のビルド成果物の配信。

use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

/// `frontend/build/` は SvelteKit (adapter-static, SPA fallback: `index.html`) の
/// ビルド成果物を配置する場所。release ビルド時にバイナリへ埋め込まれる
/// (debug ビルドではディスクから直接読む)。
#[derive(RustEmbed)]
#[folder = "frontend/build/"]
struct Assets;

const INDEX_HTML: &str = "index.html";

/// API 以外の全リクエストを受けるフォールバックハンドラ。
/// - 埋め込みアセットに一致するパスがあればそれを返す
/// - 一致しなければ SPA のエントリーポイントとして index.html を返す
pub async fn handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { INDEX_HTML } else { path };

    match Assets::get(path) {
        Some(file) => serve(path, file),
        None => match Assets::get(INDEX_HTML) {
            Some(file) => serve(INDEX_HTML, file),
            None => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

fn serve(path: &str, file: rust_embed::EmbeddedFile) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache_control = if path.starts_with("_app/immutable/") {
        // content-hash 付きファイルは恒久キャッシュ可能
        "public, max-age=31536000, immutable"
    } else {
        // index.html など、更新をすぐ反映したいもの
        "no-cache"
    };

    (
        [
            (header::CONTENT_TYPE, mime.as_ref().to_string()),
            (header::CACHE_CONTROL, cache_control.to_string()),
        ],
        file.data,
    )
        .into_response()
}
