Build a Rust + Leptos visual workspace exposed as an MCP server for Claude and Codex. This issue tracks the original stories and the expanded interaction backlog. A checked item means the acceptance criteria have been verified, not merely scaffolded.

## Delivery sequence

1. **Foundation:** shared canvas schema, Turso persistence through Toasty, MCP stdio and Streamable HTTP, Leptos editor, revision-checked atomic mutations, strict scoped edits, zoom, and accessible selection.
2. **Editor:** primitives, connectors, freehand input, voice review, formatting, layers, duplication, and conflict-aware undo.
3. **AI workflows and outputs:** process/timeline mapping, thematic grouping, reviewable gap suggestions, documents, slides, PNG, and interchange formats. Claude/Codex supplies the model; the server supplies state and mutation tools.
4. **Collaboration and governance:** authenticated users, permissions, invitations, comments, live co-editing, following, and organization policy.
5. **Host integration:** use the openable browser URL wherever native embedding or Artifacts entry points are unavailable. Third-party handoff adapters are out of scope.

## Original stories

- [x] **US-01** — Spawn a canvas from a conversational prompt.
- [ ] **US-02** — Create a canvas from a dedicated artifact entry point, subject to host extension support.
- [x] **US-03** — Capture a dictated brain dump and ask the assistant to structure it.
- [x] **US-04** — Type notes and sketch directly on the canvas.
- [x] **US-05** — Embed images and video alongside diagrams.
- [x] **US-06** — Map plans into timelines/processes with owners and dates.
- [x] **US-07** — Regroup notes by theme and draft per-person actions.
- [x] **US-08** — Inspect workflow gaps and place reviewable suggestions.
- [x] **US-09** — Restrict AI changes to a selected canvas region.
- [x] **US-10** — Co-edit live with teammates.
- [x] **US-11** — Follow another participant's viewport.
- [x] **US-12** — Invite internal/external collaborators by email.
- [ ] **US-13** — Administer feature availability and external sharing policy.
- [ ] **US-14** — Autosave and reopen canvases across devices; host Artifacts integration depends on host APIs.
- [x] **US-15** — Generate structured documents and presentation decks.
- [x] **US-16 — Cancelled by user:** Lucid, Miro, and Zoho Vani handoff is removed from scope.
- [x] **US-17** — Export a canvas as PNG.

## Expanded interaction stories

- [x] [US-18: Side-by-side workspace layout](https://github.com/robot-head/pinboard-mcp/issues/1)
- [x] [US-19: Visual differentiation of node types](https://github.com/robot-head/pinboard-mcp/issues/2)
- [x] [US-20: Dependency connector routing](https://github.com/robot-head/pinboard-mcp/issues/3)
- [x] [US-21: Zoom and fit-to-window controls](https://github.com/robot-head/pinboard-mcp/issues/4)
- [x] [US-22: Single-key shape and line creation](https://github.com/robot-head/pinboard-mcp/issues/5)
- [x] [US-23: Freehand ink and sketching](https://github.com/robot-head/pinboard-mcp/issues/6)
- [x] [US-24: Layer ordering and precision nudging](https://github.com/robot-head/pinboard-mcp/issues/7)
- [x] [US-25: Contextual formatting toolbar](https://github.com/robot-head/pinboard-mcp/issues/8)
- [x] [US-26: Instant undo and duplicate](https://github.com/robot-head/pinboard-mcp/issues/9)
- [x] [US-27: Marquee region selection for AI prompts](https://github.com/robot-head/pinboard-mcp/issues/10)
- [x] [US-28: Visual insertion of AI gap suggestions](https://github.com/robot-head/pinboard-mcp/issues/11)
- [x] [US-29: Interactive voice capture state](https://github.com/robot-head/pinboard-mcp/issues/12)
- [x] [US-30: Toggleable comment pins](https://github.com/robot-head/pinboard-mcp/issues/13)
- [x] [US-31: One-click viewport syncing](https://github.com/robot-head/pinboard-mcp/issues/14)
- [ ] [US-32: Hierarchical keyboard and screen-reader traversal](https://github.com/robot-head/pinboard-mcp/issues/15)

## Database decision

Use **Turso through Toasty 0.11** for typed reads, writes, transactions, and versioned migrations. Application code must not issue raw SQL queries. Hosted deployments read `TURSO_DATABASE_URL` and `TURSO_AUTH_TOKEN` on the Rust server; credentials must never be sent to the Leptos client. Embedded Turso is available for local development without a cloud account.

## Configurable services

Team identity and invitation email must be configurable through standard OIDC and/or SAML 2.0 sign-in plus SMTP settings, without a hard-coded provider.

## Architecture constraints

- Keep the Rust domain/mutation layer independent of MCP, HTTP, and UI transports.
- Never silently overwrite concurrent edits; use revisions, atomic batches, and a documented conflict/rebase path.
- Server-side selection checks must protect untouched nodes and connector/container effects.
- Treat presence/camera state as ephemeral, separate from durable board history.
- Native Artifacts tiles, organization settings, and cross-device host persistence are not granted by generic MCP; document supported host routes and fallbacks.
- Do not call unauthenticated local co-editing an enterprise sharing/governance implementation.


## Identity addition

- [x] [Configurable SAML sign-in alongside OIDC](https://github.com/robot-head/pinboard-mcp/issues/17)

## Validation boundaries

The implementation supplies Pinboard-owned canvas entry points, a saved library, organization settings, and authenticated persistence across browser/device sessions. Generic MCP cannot create native Claude/Codex Artifacts tiles, modify host organization settings, or save into host-owned artifact storage. Those native-host portions of US-02, US-13, and US-14 remain dependent on supported host extension APIs.

Keyboard navigation and the Chromium accessibility tree are exercised automatically. Spoken screen-reader validation with a real assistive-technology user remains required for the final acceptance of US-32. Local fixture tests validate SAML/OIDC/SMTP behavior without real vendor accounts; each deployed identity/email configuration still needs its own connection check.

## Verified implementation

- Rust: 33 passing domain/server tests, formatting, and strict Clippy checks for the native server and WASM client.
- MCP: stdio and Streamable HTTP initialization, all 11 tools, resource access, scoped edits, atomic conflicts, proposals/undo, exports, and comments.
- Browser: desktop/mobile creation/editing, live synchronization/following, PNG/reload; expanded shape/IME/format/group/ink/layer/undo/region/voice/focus coverage and Chromium AX hierarchy assertions.
- Configurable services: signed SAML positive/negative/replay fixtures, SAML-only auth, OIDC PKCE/nonce/claims, cross-provider account linking, local SMTP invitation delivery, role enforcement, comment pins, cross-size following, revocation, API tokens, and organization policy.

The unchecked original stories have browser implementations but retain a native-host dependency. US-32 retains a manual spoken screen-reader validation requirement. User-cancelled US-16 is checked only to record its removal from scope.
