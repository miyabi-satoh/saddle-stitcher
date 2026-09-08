English | [日本語](./README.ja.md)

# saddle-stitcher

A tool that turns an A4 PDF into an A3 spread PDF laid out for double-sided saddle-stitch
booklet printing. A refactor of
[Tauri-NextTS-SaddleStitcher](https://github.com/miyabi-satoh/Tauri-NextTS-SaddleStitcher)
(Tauri + Next.js + Python/PyPDF2) using
[rustvelte](https://github.com/miyabi-satoh/rustvelte) (a starter that embeds an axum
backend and a SvelteKit frontend into a single binary).

## Changes from the old Tauri-NextTS-SaddleStitcher

- The page-reordering and spread-merging logic moved from Python (PyPDF2) to native Rust
  ([lopdf](https://crates.io/crates/lopdf)). The old first-run setup dance (creating a
  Python venv and `pip install`ing `wheel`/`PyPDF2`/`pycryptodome`) is gone entirely
- File I/O moved from Tauri's native file dialogs to a plain browser `<input type="file">`
  upload plus a browser download. The "open the file after converting" checkbox was
  dropped as a result (what happens after a browser download is up to the browser/OS)
- Encrypted PDFs that decrypt with an empty password are handled automatically by lopdf,
  so the cases that used to require `pycryptodome` now need no extra package. PDFs that
  need an actual (non-empty) password are still unsupported (they were effectively
  unsupported in the old version too)
- Assumes every page is the same size (the first page's `MediaBox` is reused for the whole
  document). The page-reordering algorithm itself is a verbatim port of the old
  `SaddleStitcher.py`; its correctness was not re-verified or "fixed" (the old README
  itself notes that the right-open page order was never confirmed)
- Pages with `/Rotate` (a display-rotation flag) are rejected with an explicit error,
  rather than silently producing a wrongly-oriented output (the Form-XObject-based
  placement can't reproduce the rotation). Per-page decompressed content is also capped
  at 100MiB as a defense against decompression bombs

## Usage

Start the server with `just run` (or the distributed binary) and open
`http://127.0.0.1:3000` in a browser. Pick a PDF file and an open direction (left/right),
click "convert", and the saddle-stitch-layout PDF downloads.

The rustvelte template this project started from provides only the "wiring" for an axum
backend that embeds and serves a SvelteKit frontend as a single binary. Domain-leaning
features such as authentication and UI component libraries are intentionally left out (and
unused here too). The DB (SQLite/sqlx) connection groundwork from the template is still
present but unused by this app.

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
  - OpenAPI spec generation: utoipa (`saddle-stitcher --openapi`). `/api/v1/health` and
    `/api/v1/saddle-stitch` (the PDF conversion itself)
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
just install   # install frontend dependencies (pnpm)
```

## Development

```sh
just dev-backend   # start the backend (:3000, builds the frontend once on first run)
just dev-frontend  # start the frontend with HMR (/api is proxied to the backend)
```

## Build / Run

```sh
just build  # build the frontend, then build the release binary
just run    # build, then run the binary
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

- `[server]` `bind` / `port` (default: `127.0.0.1:3000`) / `max_upload_bytes` (max PDF
  upload size, default: 200MiB)
- `[log]` `filter` (tracing `EnvFilter` syntax; `RUST_LOG` takes precedence if set) /
  `output` (`stdout` | `file`)

## API

Everything lives under `/api/v1`. See `openapi.json` (checked in) for the spec. After
changing the API, regenerate and commit `openapi.json` and
`frontend/src/lib/api/schema.d.ts` with `just api-types`.

Errors are always returned as `{"error":{"code":"...","message":"..."}}`.

- `POST /api/v1/saddle-stitch` (`multipart/form-data`: `file`=PDF, `direction`=`left`|`right`)
  — returns the converted PDF as binary (the Japanese filename lives in
  `Content-Disposition`'s `filename*=UTF-8''...`). Default upload limit is 200MiB
  (change via `config.toml`'s `[server] max_upload_bytes`)

## Other commands

```sh
just openapi    # generate openapi.json
just api-types  # generate frontend TypeScript types from openapi.json
just fmt        # format code (cargo fmt + prettier)
just lint       # lint (clippy + eslint/prettier check)
just check      # type-check (cargo check + svelte-check)
just test       # run tests (cargo test + vitest)
just ci         # run fmt-check → lint → check → test → build in sequence
just clean      # remove build output
```

You can also see the full command list with `just --list`.

## What this template does not include (out of scope)

The following are left out of this template on the assumption that individual projects add
them as needed:

- Authentication / session management (login/logout, user tables, etc.)
- UI component libraries (e.g. shadcn-svelte), Tailwind CSS
- i18n
- Concrete business-domain CRUD APIs / DB schema
