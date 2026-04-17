# Trader Terminal Trading Design

Date: 2026-04-17

## Goal

Add a terminal trading client to `trader` so an operator can inspect symbols, monitor positions and orders, and submit trading actions from a keyboard-first interface.

The first release should feel like a real trading terminal rather than a read-only status dashboard. It should support:

- real-time watchlist monitoring
- multi-panel TUI navigation
- direct order submission
- cancel and amend flows
- clear runtime-mode visibility
- stronger confirmation for live accounts than paper accounts

The design should borrow the product shape of `longbridge-terminal`, but it must remain aligned with the current `quantd`-centered architecture in this repository.

## Scope

### In Scope

- A unified terminal entrypoint named `trader`
- Both CLI and TUI user experiences in the same terminal product
- A new TUI optimized for desktop terminal usage
- Real-time-ish trading workflow using `quantd` HTTP and WebSocket APIs
- Order submission, cancellation, and amendment
- Amendment limited to price and quantity changes only
- Live-account stronger confirmation than paper-account confirmation
- Runtime mode visibility inside the terminal
- API additions in `quantd` needed to support terminal trading

### Out of Scope

- Direct SQLite reads from the terminal client
- Bypassing `quantd` by calling `db` or `exec` internals from the client
- Complex charting or full broker-style market depth
- Conditional orders, algorithmic orders, or strategy authoring
- Merging runtime-control mutation into the main trading terminal
- Full order replacement semantics that change symbol, side, or account
- A web UI redesign or replacing the existing frontend work

## Users

Primary users are internal operators who need a fast terminal workflow for monitoring symbols and acting on orders. They are expected to understand trading concepts, but the product should still explain system constraints such as `observe_only` or broker errors in clear operational language.

The terminal is not intended as a retail end-user product. It is an internal execution surface on top of the current backend.

## Product Direction

The terminal should follow a shape similar to `longbridge-terminal`:

- one user-facing binary
- both command-style CLI and full-screen TUI
- keyboard-first interaction
- trading actions treated as first-class commands, not hidden implementation details

However, unlike `longbridge-terminal`, this terminal should not talk directly to a broker SDK or local state store. `quantd` remains the single source of truth for runtime state, order actions, and synchronization outcomes.

## Product Shape

### Unified Terminal Binary

Create a unified terminal product exposed as a single binary:

- `trader`

Expected usage forms:

- `trader tui`
- `trader quote <symbol>`
- `trader order submit ...`
- `trader order cancel ...`
- `trader order amend ...`

This gives the project both an operator-facing full-screen terminal and a scriptable CLI surface.

### Why CLI And TUI Together

This product should not be TUI-only.

Reasons:

- CLI commands are easier to script, automate, and verify
- CLI and TUI can share the same client models and error mapping
- the design direction more closely matches the reference project
- some operator actions are faster to reproduce from shell history than from full-screen navigation

## Architecture Boundary

The terminal must behave as a `quantd` client.

### Required Boundary Rules

- terminal code may call `quantd` HTTP endpoints
- terminal code may connect to `quantd` WebSocket streams
- terminal code may not read SQLite directly
- terminal code may not depend on `db` implementation details for feature completeness
- terminal code may not re-implement backend trading rules locally

This keeps the service boundary clean and follows the repository rule that data access remains behind dedicated backend interfaces.

## Proposed Rust Structure

Use a small set of terminal-focused crates plus a single user-facing binary.

Recommended shape:

- `crates/terminal_client`
- `crates/terminal_tui`
- `crates/terminal_core`
- `crates/trader`

Responsibilities:

- `terminal_client`: authenticated HTTP and WebSocket client for `quantd`
- `terminal_core`: shared request types, response mapping, terminal-facing domain models, error-code mapping, formatting helpers
- `terminal_tui`: full-screen app state, panels, forms, focus model, keyboard handling
- `trader`: binary entrypoint and CLI parsing, dispatching into subcommands or TUI mode

This keeps the network client, user interaction logic, and CLI entrypoint separate enough to test independently.

## Main TUI Layout

The default TUI should open into a dense, multi-panel trading layout rather than a single list or wizard.

### Default Panels

#### 1. Watchlist And Search

Location: left rail

Purpose:

- browse watched symbols
- see latest price summary
- jump quickly to another symbol

Capabilities:

- `j/k` or arrow navigation
- `/` to search symbols
- `Enter` to promote selected symbol into the active detail context

#### 2. Quote And Mini Chart

Location: center main panel

Purpose:

- show the active symbol's detail view
- provide the context needed before ordering

Content:

- current symbol
- latest price
- change and change percent
- daily high and low
- volume summary
- compact mini-chart or bar summary

The first release should keep charting lightweight. It is enough to give directional context rather than full analytical tooling.

#### 3. Orders

Location: right upper panel

Purpose:

- make open orders highly visible
- support cancel and amend directly from selection

Content:

- open orders first
- recent history as a secondary tab or paged section
- state, side, quantity, price, age, and account markers

Primary actions:

- `c` cancel selected order
- `e` amend selected order

#### 4. Positions And Runtime

Location: right lower panel

Purpose:

- show active exposure
- keep system mode visible during trading

Content:

- current positions
- high-level exposure summary
- current runtime mode
- connectivity or synchronization warning if relevant

If the backend is in `observe_only` or degraded state, this area must make that obvious before the user attempts another trade.

#### 5. Status Bar

Location: bottom row

Purpose:

- expose current account, connection state, last refresh time, and transient errors
- provide context-sensitive hotkey hints

When the terminal enters a confirmation flow, the status bar should switch to that mode's warning or confirmation context.

## Navigation Model

The terminal should be keyboard-first and low-friction.

### Focus And Navigation

- one active panel at a time
- a visible focus treatment for the active panel
- tab-like switching between panels with predictable shortcuts
- modal popups only for actions that require input or confirmation

### Key Principles

- reading state should be lightweight and reversible
- submitting a trade should be deliberate
- dangerous actions must not hide behind ambiguous shortcuts

The terminal should feel responsive, but it should not optimize away safety on live accounts.

## Trading Workflows

### Submit Order

Expected flow:

1. operator selects a symbol from watchlist or search
2. operator presses buy or sell shortcut
3. terminal opens a trade form with account, side, quantity, order type, and price
4. terminal shows a confirmation summary before submission
5. successful submission returns focus to the order list and highlights the new order

The first release should support the minimal order set needed by the current backend. If only limit orders are fully supported, the UI should not pretend market or advanced types are available.

### Cancel Order

Expected flow:

1. operator focuses an open order
2. operator presses cancel shortcut
3. terminal shows a confirmation step
4. terminal submits cancel request
5. order state updates through WebSocket or fallback polling

### Amend Order

Expected flow:

1. operator focuses an amendable open order
2. operator presses amend shortcut
3. terminal opens a form prefilled with current quantity and price
4. operator may edit quantity and price only
5. terminal shows a before-and-after confirmation summary
6. terminal submits amend request and updates the selected order view

Amendment must be explicitly limited to:

- quantity
- limit price

The UI should make non-editable fields such as symbol, side, and account visible but locked.

## Confirmation Strategy

The confirmation model should differ by account type.

### Paper Accounts

- one standard confirmation step for submit, cancel, and amend

### Live Accounts

- stronger confirmation than paper accounts
- still optimized for speed
- no typed confirmation phrase required in the first release

Recommended live-account rule:

- action review screen
- second explicit confirm step before network submission

This matches the chosen product direction: stronger guardrails without turning the terminal into a cumbersome approval workflow.

## Backend API Additions

The current backend already exposes useful read paths, but the terminal needs explicit order management APIs instead of relying on `POST /v1/tick`.

### Read APIs

Existing or expanded reads should support:

- current orders for an account
- current positions for an account
- runtime mode visibility
- a per-symbol quote view
- a terminal overview payload for fast initial render

Recommended additions:

- `GET /v1/terminal/overview?account_id=...`
- `GET /v1/quotes/:symbol`

`GET /v1/orders?account_id=...` should remain the canonical order list endpoint, but it should be expanded if needed with terminal-friendly fields such as:

- amendable flag
- cancelable flag
- richer normalized status

### Trading APIs

Add explicit order-management routes:

- `POST /v1/orders`
- `POST /v1/orders/:order_id/cancel`
- `POST /v1/orders/:order_id/amend`

Order creation request should include at least:

- `account_id`
- `symbol`
- `side`
- `qty`
- `order_type`
- `limit_price` when applicable

Order amend request should include only:

- `qty`
- `limit_price`

The backend must reject attempts to amend symbol, side, or account.

## Real-Time Data Strategy

The terminal should use a hybrid refresh model.

### Startup

On startup, fetch the initial data needed for first render:

- terminal overview
- current active symbol quote
- watchlist symbols from terminal overview

### Live Updates

Use WebSocket for event-driven updates where available:

- order created
- order updated
- order cancelled
- order replaced
- runtime mode changed
- backend error events

Use HTTP polling for state that does not need per-event push or where the backend does not yet emit stream events:

- watchlist quote refresh
- active symbol quote refresh

### Degraded Mode

If the WebSocket disconnects:

- show connection degradation in the status bar
- continue limited polling
- do not silently present stale state as real-time state

For live accounts, the UI should surface degraded connectivity prominently before another dangerous action is taken.

## Error Handling

Errors should be grouped by operator meaning rather than shown as raw transport failures only.

### 1. Input Errors

Examples:

- invalid quantity
- missing price
- amend request with no actual change

Behavior:

- show inline form errors
- do not drop the user out of the action flow

### 2. Trading Rule Or System Rejection

Examples:

- `observe_only`
- execution guard rejection
- order no longer cancelable
- order no longer amendable

Behavior:

- show a readable terminal message
- preserve the backend `error_code`
- keep the operator aware of why the system refused the action

### 3. Network Or Service Failure

Examples:

- timeout
- WebSocket disconnect
- HTTP 502 from broker-facing path
- backend unavailable

Behavior:

- keep the last successful snapshot visible if still useful
- show a stale or disconnected warning
- avoid implying that a request succeeded when the outcome is unknown

## CLI Design

The same binary should expose scriptable commands for common actions.

### Initial Commands

- `trader tui`
- `trader quote <symbol>`
- `trader orders list --account-id ...`
- `trader order submit ...`
- `trader order cancel --order-id ...`
- `trader order amend --order-id ...`

### Output Strategy

Support both:

- human-readable table or text output
- `--json` structured output

This makes the terminal product useful in both interactive operator workflows and shell-based automation.

## Testing Strategy

### Unit Tests

Cover:

- request construction
- account confirmation policy
- error-code mapping
- TUI state transitions for focus and action flow

### API Integration Tests

Cover backend routes for:

- order creation
- order cancellation
- order amendment
- rejection in `observe_only`
- explicit rejection and status behavior across paper and live account paths

### TUI State Tests

Test the TUI as a state machine where possible:

- panel focus changes
- submit flow progression
- amend diff rendering state
- reconnect and fallback polling transitions

### End-To-End Smoke

At minimum:

- start `quantd`
- run CLI order submit against paper account
- run cancel flow
- run amend flow
- verify order list reflects the changes

## Delivery Phases

### Phase 1: Minimal Trading Loop

- add unified `trader` binary
- add shared terminal client layer
- add backend order submit, cancel, and amend APIs
- add basic multi-panel TUI scaffold
- support paper and live confirmation differences

### Phase 2: Real-Time And UX Reliability

- expand WebSocket event coverage
- add compact quote detail panel and mini-chart
- improve reconnect handling
- add CLI `--json`

### Phase 3: Terminal Polish

- richer hotkeys and focus management
- configurable watchlists
- denser order filtering and navigation polish

## Explicit Non-Goals

The first release intentionally does not attempt:

- direct DB inspection from the terminal
- advanced broker-style charting
- full order-book depth
- conditional, iceberg, TWAP, or strategy-native orders
- runtime mode mutation inside the main trading terminal
- full broker workstation parity

## Delivery Notes

This design is intentionally narrow around a single operator promise: a keyboard-first terminal that can observe market context and safely complete the core order lifecycle on top of `quantd`.

The key product trade-off is deliberate:

- copy the useful shape of `longbridge-terminal`
- do not copy its service boundary
- keep `quantd` as the backend source of truth
- make the first version excellent at submit, cancel, and amend before adding broader terminal ambition
