//! ロギングの初期化。出力先 (stdout / ファイル) とフィルタは設定で切り替える。

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{self, LogConfig, LogOutput};

const LOG_FILE_PREFIX: &str = "saddle-stitcher.log";

/// ロギングを初期化する。
///
/// - フィルタは環境変数 `RUST_LOG` があればそれを、無ければ `config.filter` を使う。
/// - 返り値の `WorkerGuard` は非同期書き込みワーカーの生存期間を握っている。drop すると
///   バッファ中のログがフラッシュされずに消えるため、プロセス終了まで保持し続けること。
pub fn init(config: &LogConfig, log_dir: &Path) -> std::io::Result<WorkerGuard> {
    let (non_blocking, guard) = match config.output {
        LogOutput::Stdout => tracing_appender::non_blocking(std::io::stdout()),
        LogOutput::File => {
            config::create_owner_only_dir(log_dir)?;
            // 日次ローテーション: saddle-stitcher.log.YYYY-MM-DD というファイル名になる。
            let file_appender = tracing_appender::rolling::daily(log_dir, LOG_FILE_PREFIX);
            tracing_appender::non_blocking(file_appender)
        }
    };

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        // ファイル出力では ANSI エスケープを付けない。
        .with_ansi(config.output == LogOutput::Stdout);

    tracing_subscriber::registry()
        .with(env_filter(&config.filter))
        .with(fmt_layer)
        .init();

    // panic はデフォルトだとコンソールに出るだけで、ファイル出力運用では誰にも見えない。
    // ログにも残す (標準のフックはターミナル起動時に有用なので残したまま呼ぶ)。
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "パニックが発生しました");
        default_hook(info);
    }));

    Ok(guard)
}

/// `RUST_LOG` があればそれを優先し、無ければ `fallback` からフィルタを作る。
/// どちらも解析できない場合は `info` にフォールバックする (ログが全く出ないよりは良い)。
fn env_filter(fallback: &str) -> EnvFilter {
    EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(fallback))
        .unwrap_or_else(|err| {
            eprintln!("ログフィルタ '{fallback}' を解析できないため info を使います: {err}");
            EnvFilter::new("info")
        })
}
