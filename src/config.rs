//! 設定ファイル (`config.toml`) の読み込みと、設定・データを置くディレクトリの解決。

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::Deserialize;

/// `directories::ProjectDirs::from` の第1・第2引数 (qualifier, organization) を
/// reverse-domain 形式でまとめたもの。`.` の前後で分割して渡す
/// (2つの `&str` 定数に分けていると、scaffold 時の文字列置換が定数単位に効かずズレる恐れがあるため、
/// 1つの文字列にして置換対象を1箇所にまとめている)。
/// macOS: `com.amiiby.saddle-stitcher` / Windows: `example\saddle-stitcher` / Linux: `saddle-stitcher` に解決される。
/// 一度リリースした後に変更すると、既存ユーザーの設定・DB ファイルの配置場所が変わってしまうため注意。
const APP_BUNDLE_PREFIX: &str = "com.amiiby";
const APP_APPLICATION: &str = "saddle-stitcher";

/// この環境変数が設定されていれば、設定・データ・ログをすべてそのディレクトリ直下に置く。
/// Docker / systemd のように HOME が無い、あるいは配置場所を固定したい運用向け。
pub const HOME_ENV: &str = "SADDLE_STITCHER_HOME";

const CONFIG_FILE_NAME: &str = "config.toml";
const DB_FILE_NAME: &str = "saddle-stitcher.db";
const LOG_DIR_NAME: &str = "logs";
#[cfg(feature = "tray")]
const LOCK_FILE_NAME: &str = "saddle-stitcher.lock";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "設定・データディレクトリを解決できませんでした (HOME が無い環境では {HOME_ENV} を設定してください)"
    )]
    NoHomeDir,
    #[error("設定ファイルの読み込みに失敗しました: {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("設定ファイルの解析に失敗しました: {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
}

/// 設定ファイル・DB・ログの置き場所。
#[derive(Debug, Clone)]
pub struct AppDirs {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
}

impl AppDirs {
    /// 1. `SADDLE_STITCHER_HOME` が設定されていれば config_dir = data_dir = その値
    /// 2. それ以外は OS 標準のアプリデータディレクトリ (`ProjectDirs`)
    pub fn resolve() -> Result<Self, Error> {
        if let Some(home) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            let home = PathBuf::from(home);
            return Ok(Self {
                config_dir: home.clone(),
                data_dir: home,
            });
        }

        // "com.amiiby" のようなreverse-domain文字列を qualifier/organization に分割する。
        let (qualifier, organization) = APP_BUNDLE_PREFIX
            .split_once('.')
            .unwrap_or(("", APP_BUNDLE_PREFIX));
        let dirs =
            ProjectDirs::from(qualifier, organization, APP_APPLICATION).ok_or(Error::NoHomeDir)?;
        Ok(Self {
            config_dir: dirs.config_dir().to_path_buf(),
            data_dir: dirs.data_local_dir().to_path_buf(),
        })
    }

    pub fn config_path(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE_NAME)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(DB_FILE_NAME)
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join(LOG_DIR_NAME)
    }

    /// シングルインスタンス化 (`single_instance` モジュール) 用のロックファイルの場所。
    #[cfg(feature = "tray")]
    pub fn lock_path(&self) -> PathBuf {
        self.data_dir.join(LOCK_FILE_NAME)
    }
}

/// ディレクトリを作成し、Unix では所有者のみ読み書き・実行可能 (0700) にする。
/// DB・ログはいずれも機微な情報を含み得るため、親ディレクトリ自体も
/// group/other から読めないようにする。
///
/// 新規作成時は `DirBuilder::mode` で mkdir(2) 自体に 0700 を指定するため、
/// 作成直後の一瞬だけ umask 依存の緩い権限が晒される、という時間差を作らない
/// (`create_dir_all` してから `set_permissions` で上書きする方式だと、その間
/// group/other から読めてしまう恐れがある)。既に存在するディレクトリは
/// (`DirBuilder` が権限を変更しないため) `set_permissions` で明示的に矯正する。
#[cfg(unix)]
pub fn create_owner_only_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    if path.is_dir() {
        return std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
    }

    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

#[cfg(not(unix))]
pub fn create_owner_only_dir(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind: IpAddr,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        // 安全側に倒し、既定では loopback (自分自身から) のみで待ち受ける。
        // LAN 上の他端末等からアクセスさせたい場合は `0.0.0.0` 等を明示指定する。
        Self {
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 3000,
        }
    }
}

impl ServerConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.bind, self.port)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// 標準出力。systemd / Docker 等がログを回収する運用向け。
    #[default]
    Stdout,
    /// `AppDirs::log_dir()` 配下に日次ローテーションで出力する。
    File,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LogConfig {
    /// `tracing_subscriber::EnvFilter` の書式。環境変数 `RUST_LOG` があればそちらを優先する。
    pub filter: String,
    pub output: LogOutput,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            filter: "saddle_stitcher=debug,tower_http=debug".to_string(),
            // tray 常駐時は Windows でコンソールを切り離す (`tray::run` の `detach_console`)。
            // その状態で stdout を既定にすると、config.toml が無い初回起動時にログが
            // 誰にも見えなくなる。tray feature 有効時のみ既定をファイル出力にする
            // (`config.toml` で明示的に `output = "stdout"` を指定すれば上書きできる)。
            #[cfg(feature = "tray")]
            output: LogOutput::File,
            #[cfg(not(feature = "tray"))]
            output: LogOutput::Stdout,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub log: LogConfig,
}

impl Config {
    /// `dirs.config_path()` を読み込む。
    /// ファイルが存在しない場合は `Config::default()` を返す (初回起動時に無設定で動くように)。
    pub fn load(dirs: &AppDirs) -> Result<Self, Error> {
        Self::load_from(&dirs.config_path())
    }

    fn load_from(path: &Path) -> Result<Self, Error> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };

        toml::from_str(&text).map_err(|source| Error::Parse {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_file_parses_to_defaults() {
        let text = include_str!("../config.example.toml");
        let config: Config = toml::from_str(text).expect("config.example.toml should parse");
        assert_eq!(config.server.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.log.filter, LogConfig::default().filter);
        assert_eq!(config.log.output, LogOutput::Stdout);
    }

    #[test]
    fn empty_input_uses_defaults() {
        let config: Config = toml::from_str("").expect("empty input should use defaults");
        assert_eq!(config.server.socket_addr().port(), 3000);
        #[cfg(not(feature = "tray"))]
        assert_eq!(config.log.output, LogOutput::Stdout);
        #[cfg(feature = "tray")]
        assert_eq!(config.log.output, LogOutput::File);
    }

    #[test]
    fn partial_server_keeps_bind_default() {
        let config: Config = toml::from_str("[server]\nport = 1\n").unwrap();
        assert_eq!(config.server.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.server.port, 1);
    }

    #[test]
    fn unknown_field_is_rejected() {
        let result: Result<Config, _> = toml::from_str("[server]\nprot = 8080\n");
        assert!(result.is_err());
    }

    #[test]
    fn unknown_log_output_is_rejected() {
        let result: Result<Config, _> = toml::from_str("[log]\noutput = \"syslog\"\n");
        assert!(result.is_err());
    }

    #[test]
    fn missing_file_falls_back_to_default() {
        let config = Config::load_from(Path::new("does/not/exist/config.toml")).unwrap();
        assert_eq!(config.server.port, 3000);
    }

    #[test]
    fn parse_error_keeps_line_number_detail() {
        let dir = std::env::temp_dir().join(format!(
            "saddle-stitcher-config-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[server]\nport = \"abc\"\n").unwrap();

        let err = Config::load_from(&path).expect_err("invalid port should fail to parse");
        match &err {
            Error::Parse { source, .. } => {
                assert!(source.to_string().contains("line 2"), "{source}");
            }
            other => panic!("expected Error::Parse, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn app_dirs_derive_paths_from_data_dir() {
        let dirs = AppDirs {
            config_dir: PathBuf::from("/cfg"),
            data_dir: PathBuf::from("/data"),
        };
        assert_eq!(dirs.config_path(), Path::new("/cfg/config.toml"));
        assert_eq!(dirs.db_path(), Path::new("/data/saddle-stitcher.db"));
        assert_eq!(dirs.log_dir(), Path::new("/data/logs"));
    }
}
