//! ログイン時の自動起動設定 (`tray` feature 有効時のみコンパイルされる)。
//!
//! OS ごとに以下を書き換える (`auto-launch` crate 経由):
//! - Windows: レジストリ `HKEY_CURRENT_USER\...\Run` に書き込む (`enable()` は
//!   `WindowsEnableMode::CurrentUser` 指定によりカレントユーザーのみを書く。
//!   crate の既定 `WindowsEnableMode::Dynamic` はまずシステム全体
//!   (`HKEY_LOCAL_MACHINE`, 要管理者権限) への書き込みを試みるが、通常の
//!   デスクトップアプリとしてはユーザー単位で完結させたいため明示的に指定する。
//!   ただし `disable()` は enable_mode に関わらず HKLM 側の削除も試みる crate 側の
//!   実装になっている (失敗しても無視されるだけなので実害は無いが、「常にカレント
//!   ユーザーのみ」とは言い切れない。詳細は `tray.rs` のクリック処理のコメントを参照)
//! - macOS: `~/Library/LaunchAgents/` の plist
//! - Linux: XDG autostart の `.desktop` (`~/.config/autostart/`)。crate の既定が
//!   これなので指定しなくても同じ結果になるが、systemd ユーザー unit に切り替える
//!   実装 (`LinuxLaunchMode::Systemd`) も crate 側にはあるため、意図を明示する
//!   ために指定する
//!
//! ## 既知の制約: パスに含まれる特殊文字
//!
//! `auto-launch` 0.6.0 は Windows のレジストリ値・Linux の `.desktop` の `Exec` を、
//! 実行ファイルパスと引数を単純に空白区切りで連結して作る (クォートしない)。
//! `C:\Program Files\...` のようにスペースを含むインストール先だと、そのままでは
//! コマンドラインの解釈が壊れるため、この2つの OS では `set_app_path` に渡す
//! パスを明示的にクォートして回避している。
//! - Windows: 前後をダブルクォートで囲むだけでよい。`CreateProcess` のコマンドライン
//!   解析では、ダブルクォートの直前に来るバックスラッシュだけが特殊な意味を持つが、
//!   実行ファイルパスは通常 `.exe` で終わり末尾がバックスラッシュになることはない。
//! - Linux: `.desktop` の `Exec` key は Desktop Entry Specification 独自のクォート
//!   規則を持ち、シェルのクォートより厳しい。しかもこのクォート規則は、string 型の
//!   値に共通する一般エスケープ規則 (`\` を `\\` にする等) の**内側**に位置する
//!   (仕様に明記: 一般エスケープの解除が先、クォート規則の適用は後)。つまり書き込む
//!   側は「クォート規則の適用」→「一般エスケープの適用」の順で2段階エスケープする
//!   必要がある。単純に前後を `"` で囲むだけでは壊れるため、`quote_for_desktop_entry`
//!   で処理する (詳細は同関数の doc を参照)。
//!   (<https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html>)
//!
//! `is_enabled()` はどちらの OS でもこのクォート付きパスの影響を受けない
//! (Windows: レジストリのキー名は `app_name` であり値の中身は見ない。
//! Linux: 判定は `{app_name}.desktop` というファイル名の存在確認のみで、
//! ファイルの中身 (Exec 行) は読まない)。
//!
//! macOS は `ProgramArguments` (配列) で個別に渡されるためスペース自体は問題ない
//! (クォートを足すとパス自体が変わってしまうため、macOS では足さない)。ただし
//! crate は plist 生成時に XML エスケープをしていないため、パスに `&` 等の
//! XML 予約文字を含む場合はここでは救えない (crate 側の既知の制約として残る)。

use auto_launch::{AutoLaunch, AutoLaunchBuilder, LinuxLaunchMode, WindowsEnableMode};

/// OS ごとの自動起動設定を扱うハンドルを組み立てる。
///
/// 実行ファイルパスは呼び出しのたびに `current_exe()` で取得する
/// (インストール場所が変わり得るため、ビルド時に固定しない)。
pub fn build() -> Result<AutoLaunch, Box<dyn std::error::Error + Send + Sync>> {
    let exe = std::env::current_exe()?;
    let exe_path = exe
        .to_str()
        .ok_or("実行ファイルのパスが UTF-8 ではありません")?
        .to_string();

    // モジュールdoc の「既知の制約」を参照。
    #[cfg(windows)]
    let exe_path = format!("\"{exe_path}\"");
    #[cfg(target_os = "linux")]
    let exe_path = quote_for_desktop_entry(&exe_path);

    let auto = AutoLaunchBuilder::new()
        .set_app_name("saddle-stitcher")
        .set_app_path(&exe_path)
        .set_linux_launch_mode(LinuxLaunchMode::XdgAutostart)
        .set_windows_enable_mode(WindowsEnableMode::CurrentUser)
        .build()?;
    Ok(auto)
}

/// Desktop Entry Specification の `Exec` key クォート規則に従い、ダブルクォートで
/// 囲んだ引数としてエスケープする。
/// <https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html>
///
/// 2段階のエスケープが必要 (仕様に明記されている: "this escape rule [= string 型共通の
/// 一般エスケープ] is applied before the quoting rule" — つまりパース時は
/// 「一般エスケープの解除 → クォート規則の適用」の順で処理されるため、書き込み側は
/// 逆順で「クォート規則を適用 → その結果に一般エスケープを適用」としなければならない)。
///
/// 1. `Exec` 固有のクォート規則: クォート内で `"` `` ` `` `$` `\` をエスケープし
///    (直前に `\` を追加)、リテラルな `%` はフィールドコード展開と衝突するため
///    `%%` にする。
/// 2. string 型の値に共通する一般エスケープ規則: バックスラッシュ (`\`) はさらに
///    `\\` にエスケープされる。手順1で追加したエスケープ用のバックスラッシュ自身も
///    ここで倍加される (例: リテラルな `\` は最終的に `\\\\` の4文字になる)。
#[cfg(target_os = "linux")]
fn quote_for_desktop_entry(s: &str) -> String {
    let mut exec_escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                exec_escaped.push('\\');
                exec_escaped.push(c);
            }
            '%' => exec_escaped.push_str("%%"),
            _ => exec_escaped.push(c),
        }
    }
    let value_escaped = exec_escaped.replace('\\', "\\\\");
    format!("\"{value_escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    // enable/disable は実際に OS 側の設定 (レジストリ・plist・.desktop) を書き換える
    // 副作用があるため、ここではテストしない。build() は current_exe() の解決と
    // ビルダーの組み立てを行うだけで、それ自体に副作用は無いので、そこだけ検証する。
    #[test]
    fn build_succeeds_for_current_exe() {
        build().expect("current_exe() should resolve and builder should construct");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn quote_for_desktop_entry_escapes_reserved_chars() {
        assert_eq!(quote_for_desktop_entry("/usr/bin/app"), "\"/usr/bin/app\"");
        // Exec のクォート規則では `"` は `\"` になり、その `\` がさらに string 型の
        // 一般エスケープで `\\` になるため、最終的には `\\"` (バックスラッシュ2個) になる。
        assert_eq!(
            quote_for_desktop_entry(r#"/path/with "quote"/app"#),
            r#""/path/with \\"quote\\"/app""#
        );
        // リテラルな `\` は Exec のクォート規則で `\\` になり、その2つの `\` が
        // それぞれ一般エスケープでさらに `\\` になるため、最終的に `\\\\` (4個) になる。
        assert_eq!(
            quote_for_desktop_entry(r"C:\weird\path"),
            r#""C:\\\\weird\\\\path""#
        );
        // `%` は一般エスケープの対象文字 (`\`) を含まないため2段階目の影響を受けない。
        assert_eq!(quote_for_desktop_entry("100% done"), "\"100%% done\"");
        assert_eq!(
            quote_for_desktop_entry("$HOME/`cmd`"),
            r#""\\$HOME/\\`cmd\\`""#
        );
    }
}
