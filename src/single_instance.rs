//! シングルインスタンス化 (`tray` feature 有効時のみコンパイルされる)。
//!
//! タスクトレイ常駐アプリはショートカットの誤操作等で多重起動されやすい。データ
//! ディレクトリ配下のロックファイルに対する OS のファイルロック (Unix: flock,
//! Windows: LockFileEx) で二重起動を検知する。ロックはプロセスが (正常終了・
//! クラッシュを問わず) 終了して該当ファイルハンドルが閉じられれば OS 側で自動的に
//! 解放されるため、ロックファイル自体の掃除は不要。

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

// `single_instance` は main.rs 側 (bin クレート) のモジュールなので、`config` は
// lib クレートから `saddle_stitcher::config` として参照する (`crate::config` ではない)。
use saddle_stitcher::config;

/// シングルインスタンスロックの取得を試みる。
///
/// `File::try_lock` (Rust 1.89 で安定化) を使うため、専用の依存クレートは不要。
/// 返り値の `File` はロックの生存期間を握っている。呼び出し元はプロセス終了まで
/// これを保持し続けること (drop するとロックが解放される)。既に別プロセスが
/// ロックを保持している場合は `Ok(None)` を返す (これはエラーではない)。
pub fn acquire(path: &Path) -> std::io::Result<Option<File>> {
    // `path` がファイル名だけ (カレントディレクトリ相対) の場合、`parent()` は
    // 空文字列の `Path` を返す。`db::connect` に倣い、その場合は
    // ディレクトリ作成をスキップする (空パスに対する作成はエラーになるため)。
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        // ロックファイルの親はデータディレクトリそのもの (機微な DB・ログと同居する)。
        // `std::fs::create_dir_all` だと umask 依存の緩い権限で作られてしまうため、
        // db/logging と同じ `create_owner_only_dir` で 0700 固定で作る。
        config::create_owner_only_dir(parent)?;
    }
    // ロック取得だけが目的でファイルの中身は使わないため、既存の内容は保持する
    // (truncate すると、ロック取得中の別プロセスがいた場合に無意味な書き込みになる)。
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(err)) => Err(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_caller_acquires_and_second_is_rejected() {
        // テスト名を含めておく: `line!()` だけだと、将来この関数のパターンを真似た
        // 別テストが増えたときに一時ディレクトリ名が衝突しうる。
        let dir = std::env::temp_dir().join(format!(
            "saddle-stitcher-single-instance-test-{}-first_caller_acquires_and_second_is_rejected",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.lock");

        let first = acquire(&path).unwrap();
        assert!(first.is_some(), "最初の取得はロックを保持できるはず");

        let second = acquire(&path).unwrap();
        assert!(
            second.is_none(),
            "ロック保持中の別ハンドルからの取得は None になるはず"
        );

        drop(first);
        let third = acquire(&path).unwrap();
        assert!(third.is_some(), "解放後は再度取得できるはず");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn acquire_creates_owner_only_parent_dir() {
        use std::os::unix::fs::PermissionsExt;

        // 親ディレクトリが未作成の状態から呼ぶ (db.rs の同種のテストと同じ狙い):
        // `create_dir_all` に差し戻すリグレッションを検知する。
        let dir = std::env::temp_dir().join(format!(
            "saddle-stitcher-single-instance-test-{}-acquire_creates_owner_only_parent_dir",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("test.lock");

        let _lock = acquire(&path).unwrap();

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "ロックファイルの親ディレクトリは所有者のみアクセス可能であるべき"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
