.DEFAULT_GOAL := help

FRONTEND_DIR := frontend
BIN_NAME := saddle-stitcher

.PHONY: help
help: ## 使えるコマンド一覧を表示
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-16s\033[0m %s\n", $$1, $$2}'

.PHONY: install
install: ## frontend の依存関係をインストール
	cd $(FRONTEND_DIR) && pnpm install

.PHONY: frontend-build
frontend-build: ## SvelteKit(SPA) をビルドして frontend/build/ に出力
	cd $(FRONTEND_DIR) && pnpm run build

.PHONY: build
build: frontend-build ## frontend をビルドしてから release バイナリをビルド(単一バイナリを生成)
	cargo build --release

.PHONY: run
run: build ## release ビルドしてバイナリを起動
	./target/release/$(BIN_NAME)

$(FRONTEND_DIR)/build:
	$(MAKE) frontend-build

.PHONY: dev-backend
dev-backend: $(FRONTEND_DIR)/build ## backend を開発モードで起動 (frontend/build/ が無ければ初回のみビルド)
	cargo run

.PHONY: dev-frontend
dev-frontend: ## frontend を開発モードで起動 (HMR, /api は backend にプロキシ)
	cd $(FRONTEND_DIR) && pnpm run dev

.PHONY: openapi
openapi: ## OpenAPI仕様(openapi.json)をサーバー起動なしで生成する
	cargo run --quiet -- --openapi > openapi.json

.PHONY: api-types
api-types: openapi ## openapi.json から frontend 用の TypeScript 型を生成する
	cd $(FRONTEND_DIR) && pnpm run generate:api-types

.PHONY: fmt
fmt: ## コードを整形する (cargo fmt + prettier)
	cargo fmt
	cd $(FRONTEND_DIR) && pnpm run format

.PHONY: fmt-check
fmt-check: ## フォーマット崩れがないか確認する (CI向け、書き換えない)
	cargo fmt --check

.PHONY: lint
lint: $(FRONTEND_DIR)/build ## Lint を実行する (clippy + eslint/prettier check)
	cargo clippy --all-targets -- -D warnings
	cd $(FRONTEND_DIR) && pnpm run lint

.PHONY: check
check: $(FRONTEND_DIR)/build ## 型検査を実行する (cargo check + svelte-check)
	cargo check
	cd $(FRONTEND_DIR) && pnpm run check

.PHONY: test
test: $(FRONTEND_DIR)/build ## テストを実行する (cargo test + vitest + playwright e2e)
	cargo test
	cd $(FRONTEND_DIR) && pnpm run test:unit -- --run && pnpm run test:e2e

.PHONY: ci
ci: fmt-check lint check test build ## CIで実行する一連のチェック (フォーマット→lint→型検査→test→build)

.PHONY: clean
clean: ## ビルド成果物を削除する
	cargo clean
	rm -rf $(FRONTEND_DIR)/build $(FRONTEND_DIR)/.svelte-kit
