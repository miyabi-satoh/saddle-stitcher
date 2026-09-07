English | [日本語](./README.ja.md)

# saddle-stitcher

A minimal starter template that provides only the "wiring" for a Rust (axum) backend that
embeds and serves a SvelteKit (SPA) frontend as a single binary.

Domain-leaning features such as authentication, DB schema, and UI component libraries are
intentionally left out. The initial state of this project is meant to be a foundation you
can start from without hesitation once you think "I want to start using a DB in the
backend" or "I want to add Tailwind or a UI kit to the frontend."

## Tech stack

- Backend: Rust + axum
  - DB: only the connection and migration groundwork for SQLite (sqlx, WAL) is set up
    (`migrations/` is still empty). Since compile-time-checked macros like `sqlx::query!`
    are not used, `DATABASE_URL`/`.env`/the `.sqlx/` offline cache are not needed (see
    `migrations/README.md` once you start using them)
  - Config: `config.toml` (the OS-standard app data directory is resolved via
    `directories`; overridable with `SADDLE_STITCHER_HOME`)
  - Logging: tracing (stdout, or daily-rotated files)
  - Error format: a common `{"error":{"code","message"}}` envelope (`src/error.rs`)
  - OpenAPI spec generation: utoipa (`saddle-stitcher --openapi`). Currently only
    `/api/v1/health`
  - The frontend build output is embedded via rust-embed and served as a single binary
- Frontend: a bare-bones setup equivalent to `sv create` (SvelteKit) + TypeScript +
  adapter-static (SPA)
  - Already added: prettier / eslint / vitest / playwright
  - UI kits (e.g. shadcn-svelte) and Tailwind are not included. Add them when needed, e.g.
    via `pnpm dlx sv add tailwindcss`
  - API client: openapi-fetch + openapi-typescript (types generated from `openapi.json`)
- Package management: pnpm (frontend)

## Setup

```sh
make install   # install frontend dependencies (pnpm)
```

## Development

```sh
make dev-backend   # start the backend (:3000, builds the frontend once on first run)
make dev-frontend  # start the frontend with HMR (/api is proxied to the backend)
```

## Build / Run

```sh
make build  # build the frontend, then build the release binary
make run    # build, then run the binary
```

## CLI

```
saddle-stitcher                 start the server
saddle-stitcher --openapi       write the OpenAPI spec (JSON) to stdout
saddle-stitcher -v | --version  print the version
```

## Task-tray residency (optional)

Enabling the `tray` feature makes the app start as a GUI app that resides in the task tray
instead of a terminal (enabled by default when generated with `create-rustvelte --tray`).

```sh
cargo build --release --features tray
```

- Tray menu: "Open" (opens in the default browser) / "Launch at login" / "Quit"
- On Windows, the console is detached at startup. The default log output also becomes a
  file only when this feature is enabled (since stdout is invisible without a console)
- "Launch at login" is OFF by default. Toggling it rewrites the OS-level autostart setting
  for the current user (Windows: the `HKEY_CURRENT_USER\...\Run` registry key, macOS: a
  LaunchAgent, Linux: an XDG autostart `.desktop` file) via the `auto-launch` crate (no
  admin privileges required). On environments where this cannot be configured (i.e. where
  reading the state or building it fails), the menu item itself becomes unclickable
- Known limitation: the `auto-launch` crate does not XML-escape when generating the macOS
  plist, so if the executable's install path contains an XML reserved character such as
  `&`, the generated LaunchAgent plist can end up malformed. (On Windows/Linux the crate
  just joins the path and arguments with spaces, which we work around by quoting the path
  ourselves; on macOS adding quotes would change the path itself, so there is no
  workaround on our side — this is a known limitation of the crate.)
- Single-instance enforcement: a lock file in the data directory detects duplicate launches
  (e.g. accidentally double-clicking a shortcut). If an instance is already running, it
  prints "already running" and exits immediately. One-shot commands like `-v`/`--openapi`
  are unaffected, since they complete before the server would start
- Single-instance enforcement uses `std::fs::File::try_lock`, an API stabilized in Rust
  1.89. This is why `rust-version` in `Cargo.toml` is set to 1.89 (an older Rust may work
  fine without the `tray` feature, but since MSRV cannot be set per feature, the whole
  package uses this value)
- Building this feature on Linux requires `libgtk-3-dev libxdo-dev
  libayatana-appindicator3-dev` (on Debian/Ubuntu) (`.github/workflows/ci.yml` installs
  these automatically)

## Where config and data live

The config file (`config.toml`), DB, and logs live in the following locations.

- If the `SADDLE_STITCHER_HOME` environment variable is set, everything lives directly under
  that directory (for setups without a HOME, like Docker/systemd, or where you want a
  fixed location)
- Otherwise, the OS-standard app data directory
  - macOS: `~/Library/Application Support/com.amiiby.saddle-stitcher/`
  - Linux: config in `~/.config/saddle-stitcher/`, data in `~/.local/share/saddle-stitcher/`
  - Windows: config in `%APPDATA%\example\saddle-stitcher\config\`, data in
    `%LOCALAPPDATA%\example\saddle-stitcher\data\`

If `config.toml` is absent, the app starts with its defaults. See `config.example.toml` for
config options and defaults.

- `[server]` `bind` / `port` (default: `127.0.0.1:3000`)
- `[log]` `filter` (tracing `EnvFilter` syntax; `RUST_LOG` takes precedence if set) /
  `output` (`stdout` | `file`)

## API

Everything lives under `/api/v1`. See `openapi.json` (checked in) for the spec. After
changing the API, regenerate and commit `openapi.json` and
`frontend/src/lib/api/schema.d.ts` with `make api-types`.

Errors are always returned as `{"error":{"code":"...","message":"..."}}`.

## Other commands

```sh
make openapi    # generate openapi.json
make api-types  # generate frontend TypeScript types from openapi.json
make fmt        # format code (cargo fmt + prettier)
make lint       # lint (clippy + eslint/prettier check)
make check      # type-check (cargo check + svelte-check)
make test       # run tests (cargo test + vitest)
make ci         # run fmt-check → lint → check → test → build in sequence
make clean      # remove build output
```

You can also see the full command list with `make help`.

## What this template does not include (out of scope)

The following are left out of this template on the assumption that individual projects add
them as needed:

- Authentication / session management (login/logout, user tables, etc.)
- UI component libraries (e.g. shadcn-svelte), Tailwind CSS
- i18n
- Concrete business-domain CRUD APIs / DB schema
