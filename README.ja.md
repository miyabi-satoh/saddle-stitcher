[English](./README.md) | 日本語

# saddle-stitcher

Rust (axum) の backend が SvelteKit (SPA) の frontend を単一バイナリに埋め込んで配信する、
という「配線」だけを揃えた最小スターターテンプレート。

認証・DB スキーマ・UI コンポーネントライブラリ等のドメイン寄りの機能は意図的に含めていない。
「backend で DB を使い始めたい」「frontend に Tailwind や UI キットを足したい」と思った時に
迷わず始められるだけの土台、というのがこのプロジェクトの初期状態。

## 技術スタック

- backend: Rust + axum
  - DB: SQLite (sqlx, WAL) の接続・マイグレーション基盤のみ用意 (`migrations/` はまだ空)。
    `sqlx::query!` 系のコンパイル時チェックマクロは使っていないので `DATABASE_URL`/`.env`/
    `.sqlx/` オフラインキャッシュは不要 (使い始めたら `migrations/README.md` を参照)
  - 設定: `config.toml` (`directories` で OS 標準のアプリデータディレクトリを解決、
    `SADDLE_STITCHER_HOME` で上書き可)
  - ロギング: tracing (stdout またはファイルへ日次ローテーション)
  - エラー形式: `{"error":{"code","message"}}` の共通 envelope (`src/error.rs`)
  - OpenAPI 仕様生成: utoipa (`saddle-stitcher --openapi`)。今は `/api/v1/health` のみ
  - frontend ビルド成果物は rust-embed で埋め込み、単一バイナリとして配信
- frontend: `sv create`(SvelteKit) 相当のまっさらな構成 + TypeScript + adapter-static(SPA)
  - 追加済み: prettier / eslint / vitest / playwright
  - UI キット (shadcn-svelte 等) や Tailwind は含めていない。必要になったら
    `pnpm dlx sv add tailwindcss` 等で追加する
  - API クライアント: openapi-fetch + openapi-typescript (`openapi.json` から型生成)
- パッケージ管理: pnpm (frontend)

## セットアップ

```sh
make install   # frontend の依存関係をインストール (pnpm)
```

## 開発

```sh
make dev-backend   # backend を起動 (:3000, 初回のみ frontend をビルド)
make dev-frontend  # frontend を HMR 付きで起動 (/api は backend にプロキシ)
```

## ビルド・実行

```sh
make build  # frontend をビルドしてから release バイナリをビルド
make run    # build してバイナリを起動
```

## CLI

```
saddle-stitcher                 サーバーを起動する
saddle-stitcher --openapi       OpenAPI 仕様 (JSON) を標準出力に書き出す
saddle-stitcher -v | --version  バージョンを表示する
```

## タスクトレイ常駐 (任意)

`tray` feature を有効にすると、ターミナルではなくタスクトレイに常駐する GUI アプリとして
起動する (`create-rustvelte --tray` で生成した場合はこれが既定で有効)。

```sh
cargo build --release --features tray
```

- トレイメニュー: 「開く」(既定のブラウザで開く) / 「ログイン時に起動」/ 「終了」
- Windows では起動時にコンソールを切り離す。ログの既定出力もこの feature の
  有効時のみファイルになる (コンソールが無いと stdout は誰にも見えないため)
- 「ログイン時に起動」は既定で OFF。トグルすると OS 側の自動起動設定
  (Windows: レジストリ `HKEY_CURRENT_USER\...\Run`、macOS: LaunchAgent、
  Linux: XDG autostart の `.desktop`) をカレントユーザー単位で書き換える
  (`auto-launch` crate 経由、管理者権限は要らない)。設定できない環境
  (状態取得や組み立て自体に失敗する場合) ではこの項目自体がクリックできなくなる
- 既知の制約: `auto-launch` crate は macOS の plist 生成時に XML エスケープを
  行わないため、実行ファイルのインストールパスに `&` 等の XML 予約文字が
  含まれていると、生成される LaunchAgent の plist が壊れる可能性がある
  (Windows/Linux は crate がパスと引数を空白区切りで連結するだけなのに対して
  こちら側でパスをクォートして回避しているが、macOS はクォートを足すとパス
  自体が変わってしまうため対症療法できない、crate 側の既知の制約)
- シングルインスタンス化: データディレクトリのロックファイルで多重起動を検知する
  (ショートカットの誤操作等を想定)。既に起動中の場合は「既に起動しています」と
  表示してすぐ終了する。`-v`/`--openapi` 等の単発コマンドはこの影響を受けない
  (サーバー起動より前に完結するため)
- シングルインスタンス化が使う `std::fs::File::try_lock` は Rust 1.89 で
  安定化された API。このため `Cargo.toml` の `rust-version` は 1.89 にしている
  (tray を使わない場合はもっと古い Rust でも動く可能性があるが、feature 単位で
  MSRV を分けられないためパッケージ全体をこの値にしている)
- Linux でこの feature をビルドするには `libgtk-3-dev libxdo-dev
  libayatana-appindicator3-dev` (Debian/Ubuntu の場合) が必要
  (`.github/workflows/ci.yml` は自動でインストールする)

## 設定・データの置き場所

設定ファイル (`config.toml`)・DB・ログは以下の場所に置かれる。

- 環境変数 `SADDLE_STITCHER_HOME` が設定されていれば、すべてそのディレクトリ直下
  (Docker / systemd のように HOME が無い、あるいは配置場所を固定したい運用向け)
- それ以外は OS 標準のアプリデータディレクトリ
  - macOS: `~/Library/Application Support/com.amiiby.saddle-stitcher/`
  - Linux: 設定は `~/.config/saddle-stitcher/`、データは `~/.local/share/saddle-stitcher/`
  - Windows: 設定は `%APPDATA%\example\saddle-stitcher\config\`、データは `%LOCALAPPDATA%\example\saddle-stitcher\data\`

`config.toml` が無ければ既定値で起動する。設定項目と既定値は `config.example.toml` を参照。

- `[server]` `bind` / `port` (既定: `127.0.0.1:3000`)
- `[log]` `filter` (tracing EnvFilter 書式、`RUST_LOG` があれば優先) / `output` (`stdout` | `file`)

## API

`/api/v1` 配下。仕様は `openapi.json` (コミット対象) を参照。API を変更したら
`make api-types` で `openapi.json` と `frontend/src/lib/api/schema.d.ts` を再生成してコミットする。

エラーは常に `{"error":{"code":"...","message":"..."}}` の形で返る。

## その他コマンド

```sh
make openapi    # openapi.json を生成
make api-types  # openapi.json から frontend 用の TypeScript 型を生成
make fmt        # コード整形 (cargo fmt + prettier)
make lint       # Lint (clippy + eslint/prettier check)
make check      # 型検査 (cargo check + svelte-check)
make test       # テスト実行 (cargo test + vitest)
make ci         # fmt-check → lint → check → test → build を一括実行
make clean      # ビルド成果物を削除
```

コマンド一覧は `make help` でも確認できる。

## このテンプレートに含まれないもの (スコープ外)

以下は個別プロジェクトの必要に応じて追加する想定で、このテンプレートには含めていない。

- 認証・セッション管理 (login/logout、ユーザーテーブル等)
- UI コンポーネントライブラリ (shadcn-svelte 等)・Tailwind CSS
- i18n
- 具体的な業務ドメインの CRUD API・DB スキーマ
