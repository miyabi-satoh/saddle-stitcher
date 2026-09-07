//! タスクトレイ常駐モード (`tray` feature 有効時のみコンパイルされる)。

use std::net::SocketAddr;

use axum::Router;
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tracing_appender::non_blocking::WorkerGuard;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};

#[cfg(windows)]
fn detach_console() {
    use windows::Win32::System::Console::FreeConsole;
    unsafe {
        let _ = FreeConsole();
    }
}

#[cfg(not(windows))]
fn detach_console() {}

fn make_tray_icon() -> Icon {
    // .ico ファイルを同梱せず、単色 32x32 の RGBA バッファから直接アイコンを作る。
    const SIZE: u32 = 32;
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _ in 0..(SIZE * SIZE) {
        rgba.extend_from_slice(&[0x2b, 0x6c, 0xb0, 0xff]); // 不透明な青
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("failed to build tray icon")
}

/// ブラウザで開くURL用のアドレスを返す。
/// `bind = 0.0.0.0` 等の未指定アドレスをそのまま使うとブラウザで開けないため、
/// その場合はループバックアドレスに読み替える。
fn open_addr(addr: SocketAddr) -> SocketAddr {
    if addr.ip().is_unspecified() {
        SocketAddr::new(
            if addr.is_ipv6() {
                std::net::Ipv6Addr::LOCALHOST.into()
            } else {
                std::net::Ipv4Addr::LOCALHOST.into()
            },
            addr.port(),
        )
    } else {
        addr
    }
}

/// 別スレッド・専用tokioランタイムでHTTPサーバーを起動する。
/// タスクトレイの`tao`イベントループがメインスレッドを占有するため、サーバーは
/// 別スレッドに逃がす必要がある。
///
/// `bind_tx` で bind の成否を呼び出し元に通知する。bind 失敗をこのスレッド内でログに
/// 残すだけだと、操作しても何も起きないタスクトレイだけが常駐し続けてしまう
/// (非 tray モードなら bind 失敗はプロセス終了になるのに、挙動が食い違う)。
fn spawn_server(
    addr: SocketAddr,
    app: Router,
    bind_tx: std::sync::mpsc::Sender<std::io::Result<()>>,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
        rt.block_on(async {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => {
                    // 受信側 (`run`) は bind 完了を待つだけなので、closed でも無視してよい。
                    let _ = bind_tx.send(Ok(()));
                    listener
                }
                Err(err) => {
                    let _ = bind_tx.send(Err(err));
                    return;
                }
            };
            tracing::info!(%addr, "サーバーを起動しました");
            if let Err(err) = axum::serve(listener, app).await {
                tracing::error!(%err, "サーバーが異常終了しました");
            }
        });
    });
}

enum UserEvent {
    // イベントループを起こすためだけに使う。中身は読まない。
    #[allow(dead_code)]
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

/// `log_guard` はログの非同期書き込みワーカーの生存期間を握っている。
/// `tao` のイベントループ(`event_loop.run`)は正常終了時も `-> !` で戻ってこないため、
/// `main` に持たせたままでは終了時に drop されずログが flush されない。
/// そのため所有権をここに移し、終了メニュー選択時に明示的に drop する。
pub fn run(addr: SocketAddr, log_guard: WorkerGuard, app: Router) {
    // タスクトレイ常駐モード。ターミナルから起動された場合でもそのターミナルは
    // 巻き込まず、このプロセスだけをコンソールから切り離す。
    detach_console();

    let (bind_tx, bind_rx) = std::sync::mpsc::channel();
    spawn_server(addr, app, bind_tx);

    // bind が完了するまで待ってからトレイを表示する。ここで待たずに進むと、ポート
    // 使用中などで bind に失敗した場合でも、操作しても何も起きないタスクトレイだけが
    // 常駐し続けてしまう (非 tray モードなら bind 失敗はプロセス終了になる)。
    match bind_rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            tracing::error!(%addr, %err, "サーバーのポートbindに失敗しました");
            drop(log_guard);
            std::process::exit(1);
        }
        Err(_) => {
            // サーバースレッドが送信前に panic する等でチャネルが閉じた場合。
            tracing::error!("サーバー起動スレッドとの同期に失敗しました");
            drop(log_guard);
            std::process::exit(1);
        }
    }

    let mut log_guard = Some(log_guard);

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();

    let proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Tray(event));
    }));

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));

    let open_item = MenuItem::new("開く", true, None);

    // ログイン時の自動起動。`AutoLaunch` の組み立てや現在状態の取得に失敗する環境
    // (実行ファイルパスが取得できない、ホームディレクトリが解決できない等) では、
    // この項目をクリックできないようにする (中途半端に有効に見えて実際には
    // 機能しない、という状態を避けるため)。
    let auto_launch = crate::autostart::build();
    if let Err(err) = &auto_launch {
        tracing::warn!(%err, "自動起動設定を利用できません");
    }
    let autostart_state = auto_launch.as_ref().ok().and_then(|auto| {
        auto.is_enabled()
            .inspect_err(|err| tracing::warn!(%err, "自動起動設定の状態を取得できませんでした"))
            .ok()
    });
    let autostart_item = CheckMenuItem::new(
        "ログイン時に起動",
        autostart_state.is_some(),
        autostart_state.unwrap_or(false),
        None,
    );

    let quit_item = MenuItem::new("終了", true, None);

    // tray_icon はメインスレッドかつイベントループ稼働中に作る必要があるため、
    // NewEvents(Init) を待ってから生成する。
    // Drop されるとアイコンが消えるため、イベントループの外に出るまで保持し続ける。
    let mut _tray_icon = None;

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::NewEvents(StartCause::Init) => {
                let menu = Menu::new();
                menu.append(&open_item).unwrap();
                menu.append(&PredefinedMenuItem::separator()).unwrap();
                menu.append(&autostart_item).unwrap();
                menu.append(&PredefinedMenuItem::separator()).unwrap();
                menu.append(&quit_item).unwrap();

                _tray_icon = Some(
                    TrayIconBuilder::new()
                        .with_icon(make_tray_icon())
                        .with_menu(Box::new(menu))
                        .with_tooltip("saddle-stitcher")
                        .build()
                        .expect("failed to build tray icon"),
                );
            }
            Event::UserEvent(UserEvent::Menu(event)) => {
                if event.id == quit_item.id() {
                    tracing::info!("終了メニューが選択されました");
                    // バッファ中のログを flush してからプロセスを終了させる。
                    drop(log_guard.take());
                    *control_flow = ControlFlow::Exit;
                } else if event.id == open_item.id() {
                    let _ = open::that(format!("http://{}", open_addr(addr)));
                } else if event.id == autostart_item.id()
                    && let Ok(auto) = &auto_launch
                {
                    // クリック直後の `is_checked()` が自動でトグルされているかは
                    // ライブラリ内部の実装依存のため信頼せず、OS 側の実際の状態を
                    // 都度取得してから反転させる (真実の源は OS 側の設定そのもの)。
                    //
                    // `is_enabled()` 自体が失敗した場合に `unwrap_or(false)` で
                    // 「無効」とみなしてしまうと、実際には有効なのに enable() を
                    // 呼んでしまいかねない (起動時に状態取得へ失敗した場合は項目自体を
                    // 無効化しているのに、実行中の失敗は挙動が食い違ってしまう)。
                    // 状態が分からない以上は変更を試みず、項目を無効化するに留める。
                    match auto.is_enabled() {
                        Ok(currently_enabled) => {
                            let result = if currently_enabled {
                                auto.disable()
                            } else {
                                auto.enable()
                            };
                            if let Err(err) = &result {
                                tracing::error!(%err, "自動起動設定の変更に失敗しました");
                            }
                            // `result` の成否に関わらず、変更後の実際の状態を読み直す
                            // (`!currently_enabled`/`currently_enabled` を仮の真実として
                            // そのまま反映しない)。理由は2つ:
                            // - Err の場合でも OS 側には部分的に反映されている可能性がある
                            //   (例: Windows の enable() は Run 値の書き込み後、
                            //   StartupApproved の更新で失敗し得る。前段の書き込みだけは
                            //   残る)。
                            // - Ok(()) の場合も、auto-launch crate の disable() は
                            //   enable_mode に関わらずまず HKLM (システム全体) の削除を
                            //   試み、アクセス拒否ならそれを無視して HKCU (カレントユーザー)
                            //   を処理する実装になっている。他プロセス等が HKLM に同名の
                            //   登録を残していて非管理者権限では削除できない場合、
                            //   「無効化に成功したのに実際はまだ有効」という状態になり得る。
                            // 読み直し自体に失敗した場合は、状態が分からない以上
                            // (クリック前と同じ方針で) 項目を無効化するに留める。
                            match auto.is_enabled() {
                                Ok(now_enabled) => autostart_item.set_checked(now_enabled),
                                Err(err) => {
                                    tracing::warn!(%err, "自動起動設定の状態確認に失敗しました");
                                    autostart_item.set_enabled(false);
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!(%err, "自動起動設定の状態取得に失敗しました");
                            autostart_item.set_enabled(false);
                        }
                    }
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unspecified_addr_becomes_loopback() {
        let addr: SocketAddr = "0.0.0.0:47821".parse().unwrap();
        assert_eq!(open_addr(addr), "127.0.0.1:47821".parse().unwrap());
    }

    #[test]
    fn loopback_addr_is_unchanged() {
        let addr: SocketAddr = "127.0.0.1:47821".parse().unwrap();
        assert_eq!(open_addr(addr), addr);
    }
}
