set windows-shell := ["cmd.exe", "/c"]

frontend_dir := "frontend"

# 使えるコマンド一覧を表示
default:
    @just --list

# frontend の依存関係をインストール
install:
    cd {{ frontend_dir }} && pnpm install

# SvelteKit(SPA) をビルドして frontend/build/ に出力
frontend-build:
    cd {{ frontend_dir }} && pnpm run build

# frontend をビルドしてから release バイナリをビルド(単一バイナリを生成)
build: frontend-build
    cargo build --release

# release ビルドしてバイナリを起動
run: build
    cargo run --release

# frontend/build が無ければビルドする (内部用)
[private]
ensure-frontend-build:
    @{{ if path_exists(justfile_directory() / frontend_dir / "build") == "true" { "echo frontend/build exists" } else { "just frontend-build" } }}

# backend を開発モードで引数付きで起動する (無指定ならサーバー起動、--openapi/-v等も渡せる。frontend/build/ が無ければ初回のみビルド)
dev-backend *args: ensure-frontend-build
    cargo run -- {{ args }}

# frontend を開発モードで起動 (HMR, /api は backend にプロキシ)
dev-frontend:
    cd {{ frontend_dir }} && pnpm run dev

# backend/frontend をまとめて起動する (concurrently でラベル付き・1ターミナルに集約)
dev:
    cd {{ frontend_dir }} && pnpm exec concurrently -n backend,frontend -c blue,green "just dev-backend" "just dev-frontend"

# OpenAPI仕様(openapi.json)をサーバー起動なしで生成する
openapi:
    cargo run --quiet -- --openapi > openapi.json

# openapi.json から frontend 用の TypeScript 型を生成する
api-types: openapi
    cd {{ frontend_dir }} && pnpm run generate:api-types

# コードを整形する (cargo fmt + prettier)
fmt:
    cargo fmt
    cd {{ frontend_dir }} && pnpm run format

# フォーマット崩れがないか確認する (CI向け、書き換えない)
fmt-check:
    cargo fmt --check

# Lint を実行する (clippy + eslint/prettier check)
lint: ensure-frontend-build
    cargo clippy --all-targets -- -D warnings
    cd {{ frontend_dir }} && pnpm run lint

# 型検査を実行する (cargo check + svelte-check)
check: ensure-frontend-build
    cargo check
    cd {{ frontend_dir }} && pnpm run check

# テストを実行する (cargo test + vitest + playwright e2e)
test: ensure-frontend-build
    cargo test
    cd {{ frontend_dir }} && pnpm run test:unit -- --run
    cd {{ frontend_dir }} && pnpm run test:e2e

# CIで実行する一連のチェック (フォーマット→lint→型検査→test→build)
ci: fmt-check lint check test build

# ビルド成果物を削除する
[unix]
clean:
    cargo clean
    rm -rf {{ frontend_dir }}/build {{ frontend_dir }}/.svelte-kit

[windows]
clean:
    cargo clean
    if exist {{ frontend_dir }}\build rmdir /s /q {{ frontend_dir }}\build
    if exist {{ frontend_dir }}\.svelte-kit rmdir /s /q {{ frontend_dir }}\.svelte-kit
