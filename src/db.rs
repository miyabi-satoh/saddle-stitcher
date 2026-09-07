//! DB 接続とマイグレーションを扱うモジュール。

use std::path::Path;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::config;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("データベースへの接続に失敗しました")]
    Connect(#[source] sqlx::Error),
    #[error("マイグレーションに失敗しました")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("データベース操作に失敗しました")]
    Sqlx(#[from] sqlx::Error),
}

/// `path` の SQLite ファイルに接続する。ファイルが無ければ作成する。
///
/// SQLite はファイル自体は自動作成するが親ディレクトリは作らないため、接続前に作成しておく。
/// 機微なデータを含み得るため、親ディレクトリと DB ファイル自体を Unix では
/// 所有者のみ読み書き可能にする。
/// WAL (Write-Ahead Logging) にしているのは、複数のブラウザタブなど複数の接続が同時に
/// 読み書きし得るため (デフォルトの DELETE モードだと書き込み中に読み取りがブロックされやすい)。
pub async fn connect(path: &Path) -> Result<SqlitePool, Error> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        config::create_owner_only_dir(parent)
            .map_err(|err| Error::Connect(sqlx::Error::Io(err)))?;
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .map_err(Error::Connect)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|err| Error::Connect(sqlx::Error::Io(err)))?;
    }

    Ok(pool)
}

/// 未適用のマイグレーション (`migrations/`) を実行する。
pub async fn migrate(pool: &SqlitePool) -> Result<(), Error> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // `#[sqlx::test]` は `migrations/` を自動適用した新規 DB を渡してくる (今はまだ空)。
    #[sqlx::test]
    async fn migrate_is_idempotent(pool: SqlitePool) {
        migrate(&pool)
            .await
            .expect("re-running migrate should be a no-op, not an error");
    }

    #[tokio::test]
    async fn connect_creates_parent_dir_and_file() {
        let dir = std::env::temp_dir().join(format!(
            "saddle-stitcher-db-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = dir.join("nested").join("test.db");

        let pool = connect(&path).await.expect("connect should succeed");
        migrate(&pool).await.expect("migrate should succeed");
        pool.close().await;

        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn connect_makes_parent_dir_and_file_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!(
            "saddle-stitcher-db-perm-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let parent = dir.join("nested");
        let path = parent.join("test.db");

        let pool = connect(&path).await.expect("connect should succeed");
        pool.close().await;

        let dir_mode = std::fs::metadata(&parent).unwrap().permissions().mode();
        assert_eq!(
            dir_mode & 0o777,
            0o700,
            "親ディレクトリは所有者のみアクセス可能であるべき"
        );

        let file_mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            file_mode & 0o777,
            0o600,
            "DB ファイルは所有者のみ読み書き可能であるべき"
        );

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
