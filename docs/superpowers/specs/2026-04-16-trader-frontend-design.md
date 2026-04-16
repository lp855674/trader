# Trader Frontend Design

Date: 2026-04-16

## Goal

Add a standalone frontend project to `trader` that gives internal team members a multi-page monitoring console for the current quant backend. The first release is read-only, requires API key login, and is optimized for desktop operational use rather than public product presentation.

The frontend should help a teammate answer these questions quickly:

- Is the system healthy and reachable?
- What runtime mode is currently active?
- What happened in the latest runtime cycle?
- Are there local positions or pending orders that need attention?
- Does the latest reconciliation snapshot indicate mismatch or degraded operation?

## Scope

### In Scope

- A new standalone frontend project inside the `trader` repository
- API key login based on `QUANTD_API_KEY`
- A sidebar-based multi-page console
- Read-only data access for current HTTP runtime and health endpoints
- Polling-based data refresh
- Shared loading, empty, partial failure, and auth-expiry states
- Basic desktop-first responsive behavior

### Out of Scope

- Any write actions such as changing runtime mode, editing allowlists, or triggering cycles
- WebSocket live updates
- User accounts, RBAC, or multi-role authorization
- Strategy analytics, charting, K-line analysis, or PnL visualization
- Public-facing marketing or customer-facing UX
- Reusing unrelated business modules from `E:\code\mark\client`

## Users

Primary users are internal team members who need to monitor the trading system but should not have to understand every backend implementation detail. The UI should therefore explain statuses in plain operational language, while still exposing raw structured details where needed for diagnosis.

## Product Direction

The frontend will be a multi-page internal control console rather than a single dashboard page. This fits the expected long-term expansion better than a one-page panel and keeps each page focused on one operational question.

The frontend should borrow the base project shape from `E:\code\mark\client`:

- `Vite + React + TypeScript`
- router-based navigation
- dashboard shell with sidebar
- separated API client and page modules
- `shadcn/ui` as the primary component foundation

It should not copy `mark/client` business concepts such as organizations, users, subscriptions, uploads, or its auth model.

## Project Placement

Create a new top-level frontend directory in the repository:

- `client/`

This keeps frontend and Rust backend in one repository, but with clear build and dependency boundaries.

## Information Architecture

The first release includes one login page and five authenticated pages.

### 1. Login

Purpose: collect and store an API key for subsequent authenticated requests.

Behavior:

- Show a simple API key input form
- On submit, validate by calling a lightweight authenticated endpoint
- On success, persist the key locally and enter the console
- On failure, show a clear invalid-key error

### 2. Dashboard

Purpose: answer "is the system broadly okay right now?"

Content:

- Current runtime mode
- Health status
- Latest cycle summary
- Latest reconciliation summary
- Allowlist enabled state and symbol count
- Position count
- Pending order count
- Recent refresh time

The dashboard is summary-first and should route users to detail pages when needed.

### 3. Runtime

Purpose: explain the current runtime control state.

Content:

- Current runtime mode
- Allowlist enabled state
- Allowlist symbols
- Latest cycle result

This page remains read-only in the first release. It explains the current state but does not offer mutation controls.

### 4. Cycle History

Purpose: help the team inspect recent automated or manual cycle outcomes over time.

Content:

- Recent cycle runs from `/v1/runtime/cycle/history`
- Status tags and timestamps
- Key result counts such as accepted, placed, and skipped
- Expandable details or structured sub-sections for per-cycle inspection

### 5. Execution

Purpose: inspect local execution state.

Content:

- Current positions
- Current pending orders
- Latest execution summary from the most recent cycle

This page should make it easy to answer whether the local execution layer believes there is active risk or unfinished work.

### 6. Reconciliation

Purpose: inspect the latest reconciliation snapshot and compare local state awareness.

Content:

- Current runtime mode
- Local positions
- Local pending orders
- Latest reconciliation status
- Snapshot metadata and timestamp

This page should make degraded or failed broker synchronization obvious.

## Layout And Navigation

Use a desktop-first admin layout:

- Fixed left sidebar for navigation
- Top status bar for environment, connectivity, refresh state, and current session indicator
- Main content area for page-specific data

### Sidebar

Navigation items:

- Dashboard
- Runtime
- Cycle History
- Execution
- Reconciliation

### Top Bar

Display:

- Current environment label when available
- API connectivity state
- Last successful refresh timestamp
- Current polling mode
- Masked API key session indicator

### Page Composition Rules

Each authenticated page should follow the same pattern:

1. A conclusion card or summary band at the top that states the most important operational takeaway in plain language
2. Primary detail cards or tables in the middle
3. Raw detail panels or structured field sections at the bottom for engineering diagnosis

This keeps the UI readable for teammates who need clarity first, while still preserving inspectability.

## Data Refresh Strategy

Use polling rather than WebSocket subscriptions in the first release.

Default polling interval:

- `15s`

User-selectable options:

- Paused
- `15s`
- `30s`
- `60s`

Rationale:

- Existing HTTP endpoints are already sufficient
- Polling reduces implementation complexity
- Read-only monitoring does not require live event streaming for the first release

## Authentication Model

Authentication is API key based, not user-account based.

### Session Rules

- The login page accepts a raw API key
- The key is stored locally on the client machine for the current browser session model selected by implementation
- All authenticated requests send `Authorization: Bearer <key>`
- If any request returns `401` or `403`, the frontend clears the stored key, redirects to login, and explains that the session is invalid or expired

The first release does not include refresh tokens, user identity, or role differentiation.

## Frontend Architecture

Use a lightweight, page-oriented React architecture.

### Recommended Stack

- Vite
- React
- TypeScript
- React Router
- `@tanstack/react-query`
- `shadcn/ui`

### Component System

Use `shadcn/ui` as the default source for foundational UI building blocks, including:

- form controls
- cards
- tables
- dialogs if needed later
- navigation primitives
- badges, alerts, and empty-state containers

Guidelines:

- prefer `shadcn/ui` primitives over custom one-off base components
- keep visual customization focused on the trader console's information density and status semantics
- avoid importing a second overlapping component system for the first release
- wrap repeated trader-specific display patterns only when there is clear domain reuse, such as status cards or summary panels

### Directory Shape

Recommended structure:

- `src/layouts`
- `src/pages`
- `src/components`
- `src/lib`
- `src/types`
- `src/hooks`
- `src/stores`

### State Management

Use `react-query` for server data:

- request caching
- polling
- stale/loading/error state handling
- retry behavior

Use a small local store only for UI concerns such as:

- sidebar expansion state
- polling interval selection
- other view-local preferences

Do not centralize backend business data into a global client store in the first release.

### API Layer

Provide explicit client functions rather than letting pages construct URLs directly.

Examples:

- `getHealth()`
- `getRuntimeMode()`
- `getRuntimeAllowlist()`
- `getLatestCycle()`
- `getCycleHistory()`
- `getExecutionState()`
- `getLatestReconciliation()`

Benefits:

- backend field changes are isolated
- page components remain declarative
- auth and error handling can be centralized

## API Mapping

The frontend should use the existing backend endpoints already described in repository documentation:

- `/health`
- `/v1/runtime/mode`
- `/v1/runtime/allowlist`
- `/v1/runtime/cycle/latest`
- `/v1/runtime/cycle/history`
- `/v1/runtime/execution-state`
- `/v1/runtime/reconciliation/latest`

The first release assumes these endpoints are the source of truth and does not require backend API expansion before starting frontend work.

## Error Handling

Handle errors by class, not ad hoc per page.

### Auth Errors

- On `401` or `403`, clear local session and redirect to login
- Show a message that the API key is invalid or expired

### Network Or Service Unavailable

- Preserve the last successful data snapshot on screen
- Show a visible stale-data warning
- Surface the last successful refresh timestamp

### Partial Page Failure

If one query fails on a multi-query page such as Dashboard:

- keep successful sections visible
- show an inline error state only for the failed section
- avoid collapsing the entire page into a full-screen error

### Empty States

Provide explicit empty states for:

- no positions
- no pending orders
- no cycle history
- no allowlist symbols

Empty tables without explanatory text are not acceptable.

## UX Language

The UI should avoid forcing users to interpret raw backend jargon first. Prefer clear operational summaries such as:

- "System reachable"
- "Latest cycle placed no orders"
- "Reconciliation reports broker connection failure"
- "Pending orders require review"

Raw field names and structured payloads can appear in detail sections, but summary surfaces should be written for internal operators rather than backend implementers.

## Testing Strategy

The first release should include enough coverage to make login, routing, and monitoring states reliable.

### Static Validation

- TypeScript type check
- lint

### Component Or Page Tests

At minimum, cover:

- successful login flow
- invalid API key handling
- auth expiry redirect
- dashboard rendering with success data
- dashboard rendering with partial failure
- dashboard rendering with empty states

### Integration Validation

Use mock API responses or local backend integration to verify:

- all authenticated pages can load
- polling updates correctly
- stale-data warning appears when requests fail after a prior success

## Responsive Behavior

The first release is desktop-first, but should still remain usable on smaller laptop and tablet widths.

Minimum expectations:

- sidebar can collapse
- tables remain scrollable rather than breaking layout
- summary cards wrap cleanly

Full mobile-first optimization is not required in this phase.

## Explicit Non-Goals

The following are intentionally deferred:

- write operations for runtime controls
- WebSocket streaming UI
- advanced analytics and charting
- user management
- multi-environment switching UX beyond basic environment display
- visual parity with generic SaaS admin templates

## Delivery Notes

This design is intentionally narrow. The first release is a trustworthy internal read-only console that makes the current Rust backend observable for a small team. It should privilege operational clarity, low implementation risk, and clean separation from the backend codebase over feature breadth.
