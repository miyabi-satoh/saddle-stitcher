#[cfg(feature = "tray")]
mod autostart;
#[cfg(feature = "tray")]
mod single_instance;
#[cfg(feature = "tray")]
mod tray;

use saddle_stitcher::config::{AppDirs, Config};
use saddle_stitcher::{cli, db, logging};
#[cfg(not(feature = "tray"))]
use tokio::net::TcpListener;

fn main() {
    if cli::handle_args() {
        return;
    }

    let dirs = AppDirs::resolve().unwrap_or_else(|err| exit_with_error(&err, None));
    let config = Config::load(&dirs).unwrap_or_else(|err| exit_with_error(&err, None));

    // `WorkerGuard` は非同期書き込みワーカーの生存期間を握っている。drop されるとバッファ中の
    // ログがフラッシュされる。逆に drop されないままプロセスが終了すると、バッファ中のログは
    // フラッシュされずに消える。
    // `std::process::exit` を経由する終了パス (異常系) ではデストラクタが走らないため、
    // `Option` で保持して各終了パスで明示的に `take()` して drop する。
    // tray 無効時、正常系では `main` の終わりまで保持し続ける。tray 有効時は `tray::run` に
    // 所有権を渡し、終了メニュー選択時に明示的に drop する (`tao` のイベントループは正常終了時も
    // `-> !` で戻ってこないため、`main` に持たせたままでは flush されない)。
    let mut log_guard = Some(
        logging::init(&config.log, &dirs.log_dir()).unwrap_or_else(|err| {
            eprintln!("ログ初期化に失敗しました: {err}");
            std::process::exit(1);
        }),
    );

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        bind = %config.server.socket_addr(),
        config_path = %dirs.config_path().display(),
        data_dir = %dirs.data_dir.display(),
        "起動します"
    );

    // タスクトレイ常駐アプリはショートカットの誤操作等で多重起動されやすい。DB へ
    // 同時アクセスさせないよう、DB 接続より前に二重起動を検知する。
    #[cfg(feature = "tray")]
    let _instance_lock = match single_instance::acquire(&dirs.lock_path()) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            tracing::error!("既に起動しています (別プロセスが常駐中です)");
            eprintln!("既に起動しています (別プロセスが常駐中です)");
            drop(log_guard.take());
            std::process::exit(1);
        }
        Err(err) => {
            tracing::error!(%err, "シングルインスタンスロックの取得に失敗しました");
            eprintln!("シングルインスタンスロックの取得に失敗しました: {err}");
            drop(log_guard.take());
            std::process::exit(1);
        }
    };

    // tray 有効時は `tray::run` (同期関数) に `Router` を渡す必要があるため、
    // DB 接続・`Router` 組み立てだけを先に一時ランタイムで済ませておく。
    let app = {
        let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
        rt.block_on(async {
            let db_path = dirs.db_path();
            let pool = db::connect(&db_path)
                .await
                .unwrap_or_else(|err| exit_with_error(&err, log_guard.take()));
            db::migrate(&pool)
                .await
                .unwrap_or_else(|err| exit_with_error(&err, log_guard.take()));
            tracing::info!(db_path = %db_path.display(), "データベースに接続しました");
            saddle_stitcher::build_app(pool, config.server.max_upload_bytes)
        })
    };

    let addr = config.server.socket_addr();

    #[cfg(feature = "tray")]
    tray::run(
        addr,
        log_guard.take().expect("log_guard is still held here"),
        app,
    );

    #[cfg(not(feature = "tray"))]
    {
        let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
        rt.block_on(async {
            let listener = TcpListener::bind(addr).await.unwrap_or_else(|err| {
                tracing::error!(%addr, %err, "サーバーのポート bind に失敗しました");
                eprintln!("サーバーのポート bind に失敗しました ({addr}): {err}");
                drop(log_guard.take());
                std::process::exit(1);
            });
            tracing::info!("listening on http://{addr}");

            if let Err(err) = axum::serve(listener, app).await {
                tracing::error!(%err, "サーバーが異常終了しました");
                drop(log_guard.take());
                std::process::exit(1);
            }
        });
    }
}

/// エラーとその原因 (source) チェーンを、ログと標準エラー出力の両方に書き出して終了する。
/// `thiserror` の `#[error(...)]` は最上位のメッセージしか出さないため、
/// `toml::de::Error` が持つ行番号等の詳細を落とさないように辿る。
/// ログはまだ有効化されていない場合もあるため、stderr にも必ず出す。
///
/// `log_guard` を受け取って `std::process::exit` の前に drop することで、非同期ログ
/// ワーカーのバッファをフラッシュしてから終了する (`log_guard` 生成前の呼び出しは `None`)。
fn exit_with_error(
    err: &(dyn std::error::Error + 'static),
    log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
) -> ! {
    let mut chain = err.to_string();
    let mut source = err.source();
    while let Some(err) = source {
        chain.push_str("\n原因: ");
        chain.push_str(&err.to_string());
        source = err.source();
    }
    tracing::error!("{chain}");
    eprintln!("{chain}");
    drop(log_guard);
    std::process::exit(1);
}
