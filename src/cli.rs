//! コマンドライン引数の処理 (サーバー起動以外のサブコマンド)。

/// コマンドライン引数を処理する。処理して呼び出し元が即終了すべきなら `true` を返す。
pub fn handle_args() -> bool {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        None => return false,
        Some("-v") | Some("--version") => {
            println!("v{}", env!("CARGO_PKG_VERSION"));
        }
        Some("--openapi") => {
            print_openapi();
        }
        Some("-h") | Some("--help") => {
            print_usage();
        }
        Some(other) => {
            eprintln!("unknown option: {other}");
            print_usage();
            std::process::exit(1);
        }
    }
    true
}

fn print_usage() {
    eprintln!(
        "使い方: saddle-stitcher [オプション]\n\
         \n\
         オプション無しでサーバーを起動する。\n\
         \n\
         -v, --version              バージョンを表示する\n\
         --openapi                  OpenAPI 仕様 (JSON) を標準出力に書き出す\n\
         -h, --help                 このヘルプを表示する"
    );
}

/// OpenAPI スキーマを JSON で標準出力に書き出す。
/// `frontend/package.json` の `generate:api-types` (openapi-typescript) が `../openapi.json` を
/// 読みに行くため、`saddle-stitcher --openapi > openapi.json` として使う想定。
fn print_openapi() {
    match crate::api::openapi().to_pretty_json() {
        Ok(json) => println!("{json}"),
        Err(err) => {
            eprintln!("OpenAPI スキーマの出力に失敗しました: {err}");
            std::process::exit(1);
        }
    }
}
