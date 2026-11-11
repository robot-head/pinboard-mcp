# Pinboard

A visual planning canvas for humans, Claude, and Codex. Rust and Leptos provide the editor; MCP exposes canvas state and edits to your assistant. **Turso through Toasty 0.11** handles all application persistence, including accounts, permissions, comments, and sessions. There are no handwritten application SQL queries.

## Run

Requirements: Rust 1.95+, Trunk 0.22.1, the WASM target, and OpenSSL/libxml2/xmlsec development libraries. On Debian/Ubuntu:

```sh
sudo apt-get install pkg-config clang libclang-dev libssl-dev libxml2-dev libxslt1-dev libxmlsec1-dev libxmlsec1-openssl xmlsec1
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.22.1 --locked
(cd crates/web && trunk build --release)
cargo run -- --serve
```

Open **http://127.0.0.1:4127**. Without sign-in configuration, loopback mode uses a trusted local owner account. Configure **OIDC, SAML, or both** for team deployments. See [identity and deployment](docs/IDENTITY.md) and [.env.example](.env.example). Environment files are examples; the server does not load them automatically.

| Option | Default / environment |
| --- | --- |
| `--serve` | HTTP/UI + Streamable HTTP MCP only; omit for stdio MCP plus the HTTP/UI server |
| `--port` | `4127` / `PINBOARD_PORT` |
| `--bind` | `127.0.0.1` / `PINBOARD_BIND` |
| `--public-url` | Listening origin / `PINBOARD_PUBLIC_URL` |
| `--db` | `data/pinboard.db` / `PINBOARD_DB` |
| `--turso-url` | `TURSO_DATABASE_URL` |
| `--dist` | `crates/web/dist` / `PINBOARD_DIST` |
| `--no-demo` | Skip the welcome canvas in a new database |

Embedded Turso works without a cloud account. For Turso Cloud, set `TURSO_DATABASE_URL` (`libsql://`, `turso://`, or `https://`) and `TURSO_AUTH_TOKEN` in the Rust server's environment. Credentials stay on the server. Automated tests exercise embedded Turso; a real Cloud database/token is needed to verify your hosted connection. Run one process per embedded database. Use HTTP MCP to attach multiple assistants to that process; a second process on the same port will fail.

## Connect Claude or Codex

With the local server running:

```sh
codex mcp add pinboard --url http://127.0.0.1:4127/mcp
```

For an authenticated server, create a revocable MCP access token in **Account → MCP access tokens**, put it in the client's environment as `PINBOARD_MCP_TOKEN`, and use:

```sh
codex mcp add pinboard --url https://canvas.example.com/mcp --bearer-token-env-var PINBOARD_MCP_TOKEN
```

Claude Desktop can launch the stdio server. Build with `cargo build --release`, then configure absolute paths:

```json
{
  "mcpServers": {
    "pinboard": {
      "command": "/absolute/path/pinboard-mcp/target/release/pinboard-mcp",
      "args": ["--db", "/absolute/path/data/pinboard.db", "--dist", "/absolute/path/pinboard-mcp/crates/web/dist"]
    }
  }
}
```

For authenticated stdio, supply `PINBOARD_STDIO_TOKEN` through the host's server environment. It uses the same personal-token permissions and revocation checks. Supply database and identity configuration there too. Logs go to stderr; stdout is reserved for MCP. Use `--serve` when the server should outlive an assistant session.

Try: “Use Pinboard to map our launch plan into a process with owners and dates. Return the canvas link.” The assistant supplies the language-model reasoning and writes editable nodes/connectors. **Ask AI** copies a request with canvas ID, revision, and selection scope for your connected assistant; it does not run a hidden model service.

| MCP tool | Purpose |
| --- | --- |
| `list_canvases` | List canvases accessible to the authenticated user |
| `create_canvas` | Create a board with initial nodes/connectors and return its browser URL |
| `read_canvas` | Read a board or rectangular world-coordinate region |
| `edit_canvas` | Apply one atomic, revision-checked operation batch |
| `propose_gap` | Place a reviewable missing step between two nodes with rationale |
| `undo_canvas` | Undo this MCP session's last action while preserving unrelated collaborator changes |
| `export_canvas` | Return Markdown, JSON, Mermaid, or standalone HTML slides |
| `read_comments` | Read discussions, optionally within a region |
| `comment_canvas` | Start an attributed discussion on a node or canvas position |
| `reply_comment` | Append an attributed reply |
| `resolve_comment` | Resolve/reopen a discussion, subject to permissions |

Accessible boards are also MCP resources at `canvas://<id>`. Every transport enforces the same organization policy and canvas roles.

```json
{
  "canvas_id": "ID_FROM_READ_CANVAS",
  "base_revision": 3,
  "scope": ["SELECTED_NODE_ID"],
  "operations": [{
    "op": "update_node",
    "id": "SELECTED_NODE_ID",
    "changes": {"owner": "Avery", "due": "2026-12-01", "group": "Launch"}
  }]
}
```

Operations include `add_node`, `update_node`, `delete_node`, `add_edge`, `update_edge`, `delete_edge`, `accept_suggestion`, and `rename`. Read the current revision before writing. On conflict, reread and intentionally rebase; do not blindly retry. A `scope` restricts all effects to selected existing IDs, including connector endpoints and container membership. It cannot rename the board or add nodes. Gap proposals require explicit expanded/unscoped authorization; `propose_gap` preserves the existing dependency until a user accepts the proposal. Acceptance rewires it atomically.

## Editor and collaboration

- Autosaved canvas library, title editing, image/video URL references, typed notes, text, shapes, milestones, frames, swimlanes, and freehand strokes.
- Anchored curved/orthogonal connectors, labels/arrows, owner/date tags, explicit nested membership, grouping, group movement, and duplication with remapped internal links.
- Contextual color/font/bold/italic/list formatting, mixed-selection values, layers, precision movement, resizing, pan/zoom/fit, full-canvas mode, and a resizable context panel.
- Multi-step author-local undo for each browser/MCP session. It preserves unrelated fields and refuses overlapping collaborator changes. History is session-local; completed actions persist in Turso.
- Live co-editing, authenticated presence, viewport following across different window sizes, reconnect recovery, and permission-revocation checks.
- Author-attributed comments/replies, resolve/reopen, unresolved counts, and toggleable keyboard-accessible pins. Deleting an anchor preserves the discussion and its last location.
- Browser speech capture with listening/paused/error states and editable transcript review before explicit submission. Audio processing uses the browser's speech provider; availability varies by browser. No audio is stored by this server.
- PNG snapshots, Markdown documents, JSON, Mermaid, and standalone HTML slide decks (printable to PDF). Video and CORS-protected images use labeled PNG placeholders. Media is embedded by URL; there is no upload/object-storage service or PPTX renderer.

| Shortcut | Action |
| --- | --- |
| V / H | Select / pan |
| R / Shift+R / O / D | Rectangle / rounded rectangle / oval / diamond; start typing immediately |
| L / Shift+L / P | Line / connector / freehand |
| N / T / F | Note / text / frame placement tool |
| Cmd/Ctrl + Plus / Minus / 0 | Zoom in / out / fit |
| Cmd/Ctrl + D / Z / A | Duplicate / undo / select all |
| Arrows / Shift+Arrows | Move selection by 10 / 1 canvas units |
| Delete / Backspace | Delete selection |
| F6 / Cmd/Ctrl+F6 | Cycle header, canvas, and contextual/drawing toolbar |
| Tab / Shift+Tab on nodes | Traverse siblings |
| Return / Escape / Shift+Return | Enter group or edit text / go up |
| Escape | Cancel gesture, stop following, close dialog, or pause microphone |

Creation shortcuts leave inputs and IME composition alone. Dialogs trap focus and restore the opener. Reduced-motion preferences are respected.

## Architecture and host support

`crates/core` owns canvas validation, geometry, hierarchy, scope, and inverse operations. `crates/web` is the Leptos WASM client. `src` supplies Axum, rmcp, typed Toasty models, authentication, permissions, SMTP, and exports. JavaScript bridges handle browser speech, clipboard, WebSocket lifecycle, and PNG rasterization.

Canvas mutations and comments update durable history inside Toasty transactions. An async write gate serializes embedded writes; immediate transactions protect read/modify/write operations. This is optimistic concurrency, not a CRDT. Presence is ephemeral and never enters undo history. WebSockets announce revisions; periodic durable reads recover missed updates.

MCP can return an openable canvas URL in Claude and Codex. Generic MCP cannot install native Artifacts tiles, modify a host's organization settings, or add host-owned cross-device storage. Pinboard supplies its own library, settings, and authenticated browser persistence. Native host entry points remain dependent on host APIs. Lucid, Miro, and Zoho Vani handoff was **cancelled by request**. See [the roadmap](docs/ROADMAP.md) for acceptance status and remaining host/manual validation.

## Validate

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --locked
cargo check --locked -p pinboard-web --target wasm32-unknown-unknown
(cd crates/web && trunk build --release)
cargo build --locked
python3 scripts/mcp-smoke.py
npm ci
npx playwright install chromium
node scripts/identity-smoke.mjs
```

The identity suite starts disposable local OIDC/SAML/SMTP fixtures and an HTTPS proxy, generates temporary test keys, and tests signed responses, negative cases, account linking, invitations, roles, comments, tokens, and policy. `xmlsec1` and `openssl` CLIs are needed. No real identity accounts or external mail are used.

With a disposable local server running at port 4127:

```sh
npm run test:e2e
node scripts/editor-smoke.mjs
python3 scripts/http-mcp-smoke.py
```

Set `CHROMIUM_PATH=/usr/bin/chromium` to use system Chromium, or `PINBOARD_TEST_URL` for another local port. Browser tests create boards and save ignored artifacts under `artifacts/`.

Migrations are generated by Toasty and applied by its versioned migration engine. `examples/generate_initial_schema.rs` and `examples/generate_collaboration_schema.rs` document generation. After a schema is deployed, add a new migration; never replace one already applied.
