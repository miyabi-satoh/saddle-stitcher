English | [日本語](./README.ja.md)

# migrations

An empty directory — no migrations yet. Add them with sqlx-cli once you start using a DB.

```sh
cargo install sqlx-cli --no-default-features --features sqlite  # if not already installed
cargo sqlx migrate add <name>
```

At startup, `db::migrate()` (in `src/db.rs`) automatically applies everything under
`./migrations`. If you use compile-time-checked macros such as `sqlx::query!`/`query_as!`,
you'll additionally need `DATABASE_URL` (`.env`) and the offline cache (`.sqlx/`, generated
by `cargo sqlx prepare`). Not needed as long as you avoid those macros (calling
`sqlx::query`/`query_as`/`query_scalar` at runtime instead).
