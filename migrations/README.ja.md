[English](./README.md) | 日本語

# migrations

まだマイグレーションが無いディレクトリ。DB を使い始めたら sqlx-cli で追加する。

```sh
cargo install sqlx-cli --no-default-features --features sqlite  # 未インストールなら
cargo sqlx migrate add <name>
```

起動時に `db::migrate()`(`src/db.rs`)が `./migrations` 配下を自動適用する。
`sqlx::query!`/`query_as!` のようなコンパイル時チェック付きマクロを使う場合は、
`DATABASE_URL`(`.env`)とオフラインキャッシュ(`.sqlx/`、`cargo sqlx prepare` で生成)が
別途必要になる。マクロを使わない (`sqlx::query`/`query_as`/`query_scalar` を実行時に呼ぶ)
うちは不要。
