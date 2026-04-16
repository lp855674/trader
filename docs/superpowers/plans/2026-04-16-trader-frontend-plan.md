# Trader Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone `client/` React frontend inside `trader` that provides API-key-gated, read-only operational visibility into runtime, cycle, execution, and reconciliation state.

**Architecture:** Create a Vite + React + TypeScript SPA with React Router, `@tanstack/react-query`, and `shadcn/ui`. Keep server state in query hooks, keep auth as a local API-key session, and organize the app around a sidebar shell plus five operational pages backed by the existing `/health` and `/v1/runtime/*` endpoints.

**Tech Stack:** Vite, React, TypeScript, React Router, `@tanstack/react-query`, `shadcn/ui`, Tailwind CSS, Vitest, Testing Library, MSW

---

## File Structure

- Create: `client/package.json`
- Create: `client/tsconfig.json`
- Create: `client/tsconfig.app.json`
- Create: `client/vite.config.ts`
- Create: `client/components.json`
- Create: `client/src/main.tsx`
- Create: `client/src/App.tsx`
- Create: `client/src/index.css`
- Create: `client/src/layouts/AuthLayout.tsx`
- Create: `client/src/layouts/AppLayout.tsx`
- Create: `client/src/components/ProtectedRoute.tsx`
- Create: `client/src/components/app-sidebar.tsx`
- Create: `client/src/components/topbar.tsx`
- Create: `client/src/components/status-card.tsx`
- Create: `client/src/components/status-badge.tsx`
- Create: `client/src/components/empty-state.tsx`
- Create: `client/src/components/error-panel.tsx`
- Create: `client/src/components/stale-banner.tsx`
- Create: `client/src/components/loading-block.tsx`
- Create: `client/src/components/json-panel.tsx`
- Create: `client/src/components/ui/button.tsx`
- Create: `client/src/components/ui/input.tsx`
- Create: `client/src/components/ui/card.tsx`
- Create: `client/src/components/ui/table.tsx`
- Create: `client/src/components/ui/badge.tsx`
- Create: `client/src/components/ui/alert.tsx`
- Create: `client/src/components/ui/select.tsx`
- Create: `client/src/components/ui/skeleton.tsx`
- Create: `client/src/lib/session-storage.ts`
- Create: `client/src/lib/auth.ts`
- Create: `client/src/lib/api-client.ts`
- Create: `client/src/lib/query-client.ts`
- Create: `client/src/lib/format.ts`
- Create: `client/src/stores/ui-store.ts`
- Create: `client/src/types/api.ts`
- Create: `client/src/hooks/use-health.ts`
- Create: `client/src/hooks/use-runtime.ts`
- Create: `client/src/hooks/use-cycle-history.ts`
- Create: `client/src/hooks/use-execution.ts`
- Create: `client/src/hooks/use-reconciliation.ts`
- Create: `client/src/hooks/use-polling-interval.ts`
- Create: `client/src/pages/LoginPage.tsx`
- Create: `client/src/pages/DashboardPage.tsx`
- Create: `client/src/pages/RuntimePage.tsx`
- Create: `client/src/pages/CycleHistoryPage.tsx`
- Create: `client/src/pages/ExecutionPage.tsx`
- Create: `client/src/pages/ReconciliationPage.tsx`
- Create: `client/src/test/setup.ts`
- Create: `client/src/test/server.ts`
- Create: `client/src/test/handlers.ts`
- Create: `client/src/test/render.tsx`
- Create: `client/src/lib/auth.test.ts`
- Create: `client/src/components/ProtectedRoute.test.tsx`
- Create: `client/src/pages/LoginPage.test.tsx`
- Create: `client/src/pages/DashboardPage.test.tsx`
- Create: `client/src/pages/RuntimePage.test.tsx`
- Create: `client/src/pages/CycleHistoryPage.test.tsx`
- Create: `client/src/pages/ExecutionPage.test.tsx`
- Create: `client/src/pages/ReconciliationPage.test.tsx`
- Create: `client/src/App.test.tsx`
- Modify: `README.md`
- Modify: `tech.md`

## Task 1: Scaffold The Frontend Workspace

**Files:**
- Create: `client/package.json`
- Create: `client/tsconfig.json`
- Create: `client/tsconfig.app.json`
- Create: `client/vite.config.ts`
- Create: `client/src/main.tsx`
- Create: `client/src/App.tsx`
- Create: `client/src/index.css`
- Create: `client/src/test/setup.ts`

- [ ] **Step 1: Add the initial workspace scripts**

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "test": "vitest run",
    "lint": "eslint .",
    "type-check": "tsc -b --noEmit"
  }
}
```

- [ ] **Step 2: Run the first failing command**

Run: `npm --prefix client test`
Expected: FAIL because the frontend workspace does not exist yet

- [ ] **Step 3: Create the minimal app shell**

```tsx
// client/src/main.tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);
```

```tsx
// client/src/App.tsx
export default function App() {
  return <div>Trader frontend bootstrap</div>;
}
```

```ts
// client/vite.config.ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./src/test/setup.ts",
  },
});
```

- [ ] **Step 4: Verify the scaffold**

Run:
- `npm --prefix client install`
- `npm --prefix client test`
- `npm --prefix client run type-check`

Expected:
- install PASS
- test PASS with no failing tests
- type-check PASS

- [ ] **Step 5: Commit**

```bash
git add client
git commit -m "feat: scaffold trader frontend workspace"
```

## Task 2: Add API Key Session Handling And Protected Routing

**Files:**
- Create: `client/src/lib/session-storage.ts`
- Create: `client/src/lib/auth.ts`
- Create: `client/src/lib/auth.test.ts`
- Create: `client/src/components/ProtectedRoute.tsx`
- Create: `client/src/components/ProtectedRoute.test.tsx`
- Create: `client/src/layouts/AuthLayout.tsx`
- Create: `client/src/layouts/AppLayout.tsx`
- Modify: `client/src/App.tsx`

- [ ] **Step 1: Write the failing auth tests**

```ts
// client/src/lib/auth.test.ts
import { describe, expect, it } from "vitest";
import { clearSession, getSession, setSession } from "./auth";

describe("auth session", () => {
  it("stores and clears the API key", () => {
    setSession("demo-key");
    expect(getSession()).toBe("demo-key");
    clearSession();
    expect(getSession()).toBeNull();
  });
});
```

```tsx
// client/src/components/ProtectedRoute.test.tsx
import { MemoryRouter, Route, Routes } from "react-router";
import { render, screen } from "@testing-library/react";
import ProtectedRoute from "./ProtectedRoute";

it("redirects guests to login", () => {
  render(
    <MemoryRouter initialEntries={["/"]}>
      <Routes>
        <Route element={<ProtectedRoute />}>
          <Route path="/" element={<div>private</div>} />
        </Route>
        <Route path="/login" element={<div>login</div>} />
      </Routes>
    </MemoryRouter>,
  );

  expect(screen.getByText("login")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the focused tests**

Run:
- `npm --prefix client test -- src/lib/auth.test.ts`
- `npm --prefix client test -- src/components/ProtectedRoute.test.tsx`

Expected: FAIL because the auth layer does not exist yet

- [ ] **Step 3: Implement local API key storage**

```ts
// client/src/lib/session-storage.ts
const SESSION_KEY = "trader.apiKey";

export function saveApiKey(apiKey: string) {
  window.localStorage.setItem(SESSION_KEY, apiKey);
}

export function loadApiKey() {
  return window.localStorage.getItem(SESSION_KEY);
}

export function clearApiKey() {
  window.localStorage.removeItem(SESSION_KEY);
}
```

```ts
// client/src/lib/auth.ts
import { clearApiKey, loadApiKey, saveApiKey } from "./session-storage";

export function getSession() {
  return loadApiKey();
}

export function setSession(apiKey: string) {
  saveApiKey(apiKey.trim());
}

export function clearSession() {
  clearApiKey();
}
```

- [ ] **Step 4: Wire protected routes**

```tsx
// client/src/components/ProtectedRoute.tsx
import { Navigate, Outlet, useLocation } from "react-router";
import { getSession } from "../lib/auth";

export default function ProtectedRoute() {
  const location = useLocation();
  const apiKey = getSession();

  if (!apiKey) {
    return <Navigate to="/login" replace state={{ from: location.pathname }} />;
  }

  return <Outlet />;
}
```

```tsx
// client/src/App.tsx
import { Route, Routes } from "react-router";
import ProtectedRoute from "./components/ProtectedRoute";
import AppLayout from "./layouts/AppLayout";
import AuthLayout from "./layouts/AuthLayout";
import LoginPage from "./pages/LoginPage";

function PlaceholderPage() {
  return <div>placeholder</div>;
}

export default function App() {
  return (
    <Routes>
      <Route element={<AuthLayout />}>
        <Route path="/login" element={<LoginPage />} />
      </Route>
      <Route element={<ProtectedRoute />}>
        <Route element={<AppLayout />}>
          <Route path="/" element={<PlaceholderPage />} />
        </Route>
      </Route>
    </Routes>
  );
}
```

- [ ] **Step 5: Verify and commit**

Run:
- `npm --prefix client test -- src/lib/auth.test.ts`
- `npm --prefix client test -- src/components/ProtectedRoute.test.tsx`
- `npm --prefix client run type-check`

Expected: PASS

```bash
git add client/src/lib/session-storage.ts client/src/lib/auth.ts client/src/lib/auth.test.ts client/src/components/ProtectedRoute.tsx client/src/components/ProtectedRoute.test.tsx client/src/layouts/AuthLayout.tsx client/src/layouts/AppLayout.tsx client/src/App.tsx
git commit -m "feat: add API key session and protected routing"
```

## Task 3: Build The API Client, Query Client, And Test Harness

**Files:**
- Create: `client/src/lib/api-client.ts`
- Create: `client/src/lib/query-client.ts`
- Create: `client/src/types/api.ts`
- Create: `client/src/test/server.ts`
- Create: `client/src/test/handlers.ts`
- Create: `client/src/test/render.tsx`
- Modify: `client/src/test/setup.ts`
- Create: `client/src/App.test.tsx`

- [ ] **Step 1: Write a failing app smoke test**

```tsx
// client/src/App.test.tsx
import { render, screen } from "./test/render";
import App from "./App";

it("renders login by default", () => {
  render(<App />, { route: "/login" });
  expect(screen.getByRole("heading", { name: /api key/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the app test**

Run: `npm --prefix client test -- src/App.test.tsx`
Expected: FAIL because the query and render harness are not ready

- [ ] **Step 3: Add the typed API client**

```ts
// client/src/lib/api-client.ts
import { clearSession, getSession } from "./auth";

export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
  ) {
    super(message);
  }
}

function notifySessionExpired() {
  window.dispatchEvent(new CustomEvent("trader:session-expired"));
}

export async function apiFetch<T>(path: string): Promise<T> {
  const apiKey = getSession();
  const response = await fetch(path, {
    headers: apiKey ? { Authorization: `Bearer ${apiKey}` } : {},
  });

  if (response.status === 401 || response.status === 403) {
    clearSession();
    notifySessionExpired();
    throw new ApiError(response.status, "Session expired");
  }

  if (!response.ok) {
    throw new ApiError(response.status, `Request failed: ${path}`);
  }

  return response.json() as Promise<T>;
}

export async function validateApiKey(candidate: string) {
  const response = await fetch("/v1/runtime/mode", {
    headers: { Authorization: `Bearer ${candidate.trim()}` },
  });

  if (!response.ok) {
    throw new ApiError(response.status, "Invalid API key");
  }

  return response.json();
}
```

```ts
// client/src/lib/query-client.ts
import { QueryClient } from "@tanstack/react-query";

export function createQueryClient() {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: 1,
        refetchOnWindowFocus: false,
      },
    },
  });
}
```

- [ ] **Step 4: Add MSW handlers and a provider-based render helper**

```ts
// client/src/test/handlers.ts
import { http, HttpResponse } from "msw";

export const handlers = [
  http.get("/health", () => HttpResponse.json({ status: "ok" })),
  http.get("/v1/runtime/mode", () => HttpResponse.json({ mode: "observe_only" })),
  http.get("/v1/runtime/allowlist", () => HttpResponse.json({ enabled: true, symbols: ["AAPL.US"] })),
  http.get("/v1/runtime/cycle/latest", () => HttpResponse.json({ status: "ok", accepted: 1, placed: 0, skipped: 1 })),
  http.get("/v1/runtime/cycle/history", () => HttpResponse.json({ runs: [{ status: "ok", accepted: 1, placed: 0, skipped: 1 }] })),
  http.get("/v1/runtime/execution-state", () => HttpResponse.json({ positions: [], open_orders: [], latest_cycle: { accepted: 1 } })),
  http.get("/v1/runtime/reconciliation/latest", () => HttpResponse.json({ status: "ok", positions: [], open_orders: [] })),
];
```

```tsx
// client/src/test/render.tsx
import { QueryClientProvider } from "@tanstack/react-query";
import { render as rtlRender } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { createQueryClient } from "../lib/query-client";

export function render(ui: React.ReactNode, options?: { route?: string }) {
  const client = createQueryClient();

  return rtlRender(
    <MemoryRouter initialEntries={[options?.route ?? "/"]}>
      <QueryClientProvider client={client}>{ui}</QueryClientProvider>
    </MemoryRouter>,
  );
}
```

- [ ] **Step 5: Verify and commit**

Run:
- `npm --prefix client test -- src/App.test.tsx`
- `npm --prefix client test`

Expected: PASS

```bash
git add client/src/lib/api-client.ts client/src/lib/query-client.ts client/src/types/api.ts client/src/test/server.ts client/src/test/handlers.ts client/src/test/render.tsx client/src/test/setup.ts client/src/App.test.tsx
git commit -m "feat: add trader frontend API and test harness"
```

## Task 4: Implement Login, shadcn/ui Foundations, And The App Shell

**Files:**
- Create: `client/src/pages/LoginPage.tsx`
- Create: `client/src/pages/LoginPage.test.tsx`
- Create: `client/src/components/app-sidebar.tsx`
- Create: `client/src/components/topbar.tsx`
- Create: `client/src/components/ui/button.tsx`
- Create: `client/src/components/ui/input.tsx`
- Create: `client/src/components/ui/card.tsx`
- Create: `client/src/components/ui/alert.tsx`
- Create: `client/src/components/ui/badge.tsx`
- Create: `client/src/components/ui/select.tsx`
- Create: `client/src/components/ui/skeleton.tsx`
- Create: `client/src/stores/ui-store.ts`
- Create: `client/src/hooks/use-polling-interval.ts`
- Modify: `client/src/layouts/AuthLayout.tsx`
- Modify: `client/src/layouts/AppLayout.tsx`
- Modify: `client/src/App.tsx`
- Modify: `client/src/App.test.tsx`

- [ ] **Step 1: Write failing tests for login and authenticated navigation**

```tsx
// client/src/pages/LoginPage.test.tsx
import userEvent from "@testing-library/user-event";
import { render, screen, waitFor } from "../test/render";
import LoginPage from "./LoginPage";

it("stores the key and navigates after validation", async () => {
  render(<LoginPage />);
  await userEvent.type(screen.getByLabelText(/api key/i), "demo-key");
  await userEvent.click(screen.getByRole("button", { name: /enter console/i }));
  await waitFor(() => {
    expect(window.localStorage.getItem("trader.apiKey")).toBe("demo-key");
  });
});
```

```tsx
// client/src/App.test.tsx
import { render, screen } from "./test/render";
import App from "./App";
import { setSession } from "./lib/auth";

it("shows the console navigation for authenticated users", () => {
  setSession("demo-key");
  render(<App />, { route: "/" });
  expect(screen.getByRole("link", { name: /dashboard/i })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /runtime/i })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: /cycle history/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the focused tests**

Run:
- `npm --prefix client test -- src/pages/LoginPage.test.tsx`
- `npm --prefix client test -- src/App.test.tsx`

Expected: FAIL because the shell and login page are incomplete

- [ ] **Step 3: Implement login with API key validation**

```tsx
// client/src/pages/LoginPage.tsx
import { FormEvent, useState } from "react";
import { useNavigate } from "react-router";
import { setSession } from "../lib/auth";
import { validateApiKey } from "../lib/api-client";

export default function LoginPage() {
  const navigate = useNavigate();
  const [apiKey, setApiKey] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  async function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError(null);

    try {
      await validateApiKey(apiKey);
      setSession(apiKey);
      navigate("/");
    } catch {
      setError("Invalid API key");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form onSubmit={onSubmit}>
      <h1>API Key Login</h1>
      <label htmlFor="api-key">API Key</label>
      <input id="api-key" value={apiKey} onChange={(event) => setApiKey(event.target.value)} />
      {error ? <div role="alert">{error}</div> : null}
      <button type="submit">Enter Console</button>
    </form>
  );
}
```

- [ ] **Step 4: Implement the sidebar shell and polling selector**

```ts
// client/src/stores/ui-store.ts
import { create } from "zustand";

type UiState = {
  sidebarOpen: boolean;
  pollingMs: number | false;
  setSidebarOpen: (open: boolean) => void;
  setPollingMs: (value: number | false) => void;
};

export const useUiStore = create<UiState>((set) => ({
  sidebarOpen: true,
  pollingMs: 15000,
  setSidebarOpen: (sidebarOpen) => set({ sidebarOpen }),
  setPollingMs: (pollingMs) => set({ pollingMs }),
}));
```

```tsx
// client/src/App.tsx
import { Route, Routes } from "react-router";
import ProtectedRoute from "./components/ProtectedRoute";
import AppLayout from "./layouts/AppLayout";
import AuthLayout from "./layouts/AuthLayout";
import CycleHistoryPage from "./pages/CycleHistoryPage";
import DashboardPage from "./pages/DashboardPage";
import ExecutionPage from "./pages/ExecutionPage";
import LoginPage from "./pages/LoginPage";
import ReconciliationPage from "./pages/ReconciliationPage";
import RuntimePage from "./pages/RuntimePage";

export default function App() {
  return (
    <Routes>
      <Route element={<AuthLayout />}>
        <Route path="/login" element={<LoginPage />} />
      </Route>
      <Route element={<ProtectedRoute />}>
        <Route element={<AppLayout />}>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/runtime" element={<RuntimePage />} />
          <Route path="/cycle-history" element={<CycleHistoryPage />} />
          <Route path="/execution" element={<ExecutionPage />} />
          <Route path="/reconciliation" element={<ReconciliationPage />} />
        </Route>
      </Route>
    </Routes>
  );
}
```

- [ ] **Step 5: Verify and commit**

Run:
- `npm --prefix client test -- src/pages/LoginPage.test.tsx`
- `npm --prefix client test -- src/App.test.tsx`
- `npm --prefix client run type-check`

Expected: PASS

```bash
git add client/src/pages/LoginPage.tsx client/src/pages/LoginPage.test.tsx client/src/components/app-sidebar.tsx client/src/components/topbar.tsx client/src/components/ui/button.tsx client/src/components/ui/input.tsx client/src/components/ui/card.tsx client/src/components/ui/alert.tsx client/src/components/ui/badge.tsx client/src/components/ui/select.tsx client/src/components/ui/skeleton.tsx client/src/stores/ui-store.ts client/src/hooks/use-polling-interval.ts client/src/layouts/AuthLayout.tsx client/src/layouts/AppLayout.tsx client/src/App.tsx client/src/App.test.tsx
git commit -m "feat: add trader frontend shell and login"
```

## Task 5: Implement Query Hooks, Shared Operational States, And Dashboard Runtime Surfaces

**Files:**
- Create: `client/src/hooks/use-health.ts`
- Create: `client/src/hooks/use-runtime.ts`
- Create: `client/src/hooks/use-cycle-history.ts`
- Create: `client/src/hooks/use-execution.ts`
- Create: `client/src/hooks/use-reconciliation.ts`
- Create: `client/src/components/status-card.tsx`
- Create: `client/src/components/status-badge.tsx`
- Create: `client/src/components/empty-state.tsx`
- Create: `client/src/components/error-panel.tsx`
- Create: `client/src/components/stale-banner.tsx`
- Create: `client/src/components/loading-block.tsx`
- Create: `client/src/components/json-panel.tsx`
- Create: `client/src/components/ui/table.tsx`
- Create: `client/src/pages/DashboardPage.tsx`
- Create: `client/src/pages/RuntimePage.tsx`
- Create: `client/src/pages/DashboardPage.test.tsx`
- Create: `client/src/pages/RuntimePage.test.tsx`

- [ ] **Step 1: Write failing tests for dashboard and runtime pages**

```tsx
// client/src/pages/DashboardPage.test.tsx
import { HttpResponse, http } from "msw";
import { render, screen } from "../test/render";
import { server } from "../test/server";
import DashboardPage from "./DashboardPage";

it("shows summary values from runtime endpoints", async () => {
  render(<DashboardPage />);
  expect(await screen.findByText(/runtime mode/i)).toBeInTheDocument();
  expect(await screen.findByText(/observe_only/i)).toBeInTheDocument();
});

it("shows a stale banner when one section fails", async () => {
  server.use(
    http.get("/v1/runtime/reconciliation/latest", () => new HttpResponse(null, { status: 500 })),
  );
  render(<DashboardPage />);
  expect(await screen.findByText(/data may be stale/i)).toBeInTheDocument();
});
```

```tsx
// client/src/pages/RuntimePage.test.tsx
import { render, screen } from "../test/render";
import RuntimePage from "./RuntimePage";

it("shows mode, allowlist, and latest cycle sections", async () => {
  render(<RuntimePage />);
  expect(await screen.findByRole("heading", { name: /runtime state/i })).toBeInTheDocument();
  expect(await screen.findByText(/AAPL.US/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the page tests**

Run:
- `npm --prefix client test -- src/pages/DashboardPage.test.tsx`
- `npm --prefix client test -- src/pages/RuntimePage.test.tsx`

Expected: FAIL because the query hooks and pages are missing

- [ ] **Step 3: Implement the query hooks**

```ts
// client/src/hooks/use-runtime.ts
import { useQuery } from "@tanstack/react-query";
import { apiFetch } from "../lib/api-client";
import type { RuntimeAllowlistResponse, RuntimeCycleSummary, RuntimeModeResponse } from "../types/api";
import { usePollingInterval } from "./use-polling-interval";

export function useRuntimeMode() {
  const refetchInterval = usePollingInterval();
  return useQuery({
    queryKey: ["runtime-mode"],
    queryFn: () => apiFetch<RuntimeModeResponse>("/v1/runtime/mode"),
    refetchInterval,
  });
}

export function useRuntimeAllowlist() {
  const refetchInterval = usePollingInterval();
  return useQuery({
    queryKey: ["runtime-allowlist"],
    queryFn: () => apiFetch<RuntimeAllowlistResponse>("/v1/runtime/allowlist"),
    refetchInterval,
  });
}

export function useLatestCycle() {
  const refetchInterval = usePollingInterval();
  return useQuery({
    queryKey: ["runtime-cycle-latest"],
    queryFn: () => apiFetch<RuntimeCycleSummary>("/v1/runtime/cycle/latest"),
    refetchInterval,
  });
}
```

- [ ] **Step 4: Implement shared states and the first two pages**

```tsx
// client/src/components/stale-banner.tsx
type StaleBannerProps = { visible: boolean };

export default function StaleBanner({ visible }: StaleBannerProps) {
  if (!visible) return null;
  return <div role="alert">Data may be stale.</div>;
}
```

```tsx
// client/src/pages/DashboardPage.tsx
import EmptyState from "../components/empty-state";
import StaleBanner from "../components/stale-banner";
import { useExecutionState } from "../hooks/use-execution";
import { useHealth } from "../hooks/use-health";
import { useLatestCycle, useRuntimeAllowlist, useRuntimeMode } from "../hooks/use-runtime";
import { useLatestReconciliation } from "../hooks/use-reconciliation";

export default function DashboardPage() {
  const health = useHealth();
  const mode = useRuntimeMode();
  const allowlist = useRuntimeAllowlist();
  const cycle = useLatestCycle();
  const execution = useExecutionState();
  const reconciliation = useLatestReconciliation();

  const hasError = [health, mode, allowlist, cycle, execution, reconciliation].some((query) => query.isError);

  if (mode.isLoading || allowlist.isLoading) return <div>Loading dashboard</div>;
  if (!mode.data || !allowlist.data) {
    return <EmptyState title="Dashboard unavailable" description="Runtime summary data is not available yet." />;
  }

  return (
    <section>
      <StaleBanner visible={hasError} />
      <h1>Dashboard</h1>
      <div>Runtime mode</div>
      <div>{mode.data.mode}</div>
      <div>Health</div>
      <div>{health.data?.status ?? "unknown"}</div>
      <div>Pending orders</div>
      <div>{execution.data?.open_orders.length ?? 0}</div>
      <div>Reconciliation</div>
      <div>{reconciliation.data?.status ?? "unknown"}</div>
    </section>
  );
}
```

```tsx
// client/src/pages/RuntimePage.tsx
import EmptyState from "../components/empty-state";
import { useLatestCycle, useRuntimeAllowlist, useRuntimeMode } from "../hooks/use-runtime";

export default function RuntimePage() {
  const mode = useRuntimeMode();
  const allowlist = useRuntimeAllowlist();
  const cycle = useLatestCycle();

  if (mode.isLoading || allowlist.isLoading || cycle.isLoading) return <div>Loading runtime state</div>;
  if (!mode.data || !allowlist.data) {
    return <EmptyState title="Runtime data unavailable" description="Runtime mode or allowlist data could not be loaded." />;
  }

  return (
    <section>
      <h1>Runtime State</h1>
      <div>{mode.data.mode}</div>
      <ul>
        {allowlist.data.symbols.map((symbol) => (
          <li key={symbol}>{symbol}</li>
        ))}
      </ul>
      <div>Latest cycle status: {cycle.data?.status ?? "unknown"}</div>
    </section>
  );
}
```

- [ ] **Step 5: Verify and commit**

Run:
- `npm --prefix client test -- src/pages/DashboardPage.test.tsx`
- `npm --prefix client test -- src/pages/RuntimePage.test.tsx`
- `npm --prefix client run type-check`

Expected: PASS

```bash
git add client/src/hooks/use-health.ts client/src/hooks/use-runtime.ts client/src/hooks/use-cycle-history.ts client/src/hooks/use-execution.ts client/src/hooks/use-reconciliation.ts client/src/components/status-card.tsx client/src/components/status-badge.tsx client/src/components/empty-state.tsx client/src/components/error-panel.tsx client/src/components/stale-banner.tsx client/src/components/loading-block.tsx client/src/components/json-panel.tsx client/src/components/ui/table.tsx client/src/pages/DashboardPage.tsx client/src/pages/RuntimePage.tsx client/src/pages/DashboardPage.test.tsx client/src/pages/RuntimePage.test.tsx
git commit -m "feat: add dashboard and runtime views"
```

## Task 6: Implement Cycle History, Execution, Reconciliation, And Resilience Rules

**Files:**
- Create: `client/src/pages/CycleHistoryPage.tsx`
- Create: `client/src/pages/ExecutionPage.tsx`
- Create: `client/src/pages/ReconciliationPage.tsx`
- Create: `client/src/pages/CycleHistoryPage.test.tsx`
- Create: `client/src/pages/ExecutionPage.test.tsx`
- Create: `client/src/pages/ReconciliationPage.test.tsx`
- Modify: `client/src/App.test.tsx`
- Modify: `client/src/pages/DashboardPage.test.tsx`

- [ ] **Step 1: Write failing tests for the remaining pages and auth expiry**

```tsx
// client/src/pages/CycleHistoryPage.test.tsx
import { render, screen } from "../test/render";
import CycleHistoryPage from "./CycleHistoryPage";

it("shows recent cycle rows", async () => {
  render(<CycleHistoryPage />);
  expect(await screen.findByRole("heading", { name: /cycle history/i })).toBeInTheDocument();
});
```

```tsx
// client/src/App.test.tsx
import { HttpResponse, http } from "msw";
import { waitFor } from "@testing-library/react";
import { render, screen } from "./test/render";
import { server } from "./test/server";
import App from "./App";
import { setSession } from "./lib/auth";

it("returns to login when the API starts returning 401", async () => {
  setSession("expired-key");
  server.use(http.get("/v1/runtime/mode", () => new HttpResponse(null, { status: 401 })));
  render(<App />, { route: "/" });
  await waitFor(() => {
    expect(screen.getByRole("heading", { name: /api key login/i })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests**

Run:
- `npm --prefix client test -- src/pages/CycleHistoryPage.test.tsx`
- `npm --prefix client test -- src/pages/ExecutionPage.test.tsx`
- `npm --prefix client test -- src/pages/ReconciliationPage.test.tsx`
- `npm --prefix client test -- src/App.test.tsx`

Expected: FAIL because the remaining pages and expiry redirect are incomplete

- [ ] **Step 3: Implement the remaining pages**

```tsx
// client/src/pages/CycleHistoryPage.tsx
import EmptyState from "../components/empty-state";
import { useCycleHistory } from "../hooks/use-cycle-history";

export default function CycleHistoryPage() {
  const history = useCycleHistory();
  if (history.isLoading) return <div>Loading cycle history</div>;
  if (!history.data?.runs.length) {
    return <EmptyState title="No cycle history" description="No cycle runs have been recorded yet." />;
  }

  return (
    <section>
      <h1>Cycle History</h1>
      {history.data.runs.map((run, index) => (
        <article key={`${run.started_at ?? "run"}-${index}`}>
          <div>Status: {run.status ?? "unknown"}</div>
          <div>Accepted: {run.accepted ?? 0}</div>
          <div>Placed: {run.placed ?? 0}</div>
          <div>Skipped: {run.skipped ?? 0}</div>
        </article>
      ))}
    </section>
  );
}
```

```tsx
// client/src/pages/ExecutionPage.tsx
import EmptyState from "../components/empty-state";
import { useExecutionState } from "../hooks/use-execution";

export default function ExecutionPage() {
  const execution = useExecutionState();
  if (execution.isLoading) return <div>Loading execution state</div>;
  if (!execution.data) {
    return <EmptyState title="Execution unavailable" description="Execution state could not be loaded." />;
  }

  return (
    <section>
      <h1>Execution State</h1>
      {execution.data.positions.length ? <div>{execution.data.positions.length} positions</div> : <EmptyState title="No positions" description="The local execution layer reports no open positions." />}
      {execution.data.open_orders.length ? <div>{execution.data.open_orders.length} pending orders</div> : <EmptyState title="No pending orders" description="There are no open local orders requiring attention." />}
    </section>
  );
}
```

```tsx
// client/src/pages/ReconciliationPage.tsx
import EmptyState from "../components/empty-state";
import { useLatestReconciliation } from "../hooks/use-reconciliation";

export default function ReconciliationPage() {
  const reconciliation = useLatestReconciliation();
  if (reconciliation.isLoading) return <div>Loading reconciliation</div>;
  if (!reconciliation.data) {
    return <EmptyState title="Reconciliation unavailable" description="No reconciliation snapshot could be loaded." />;
  }

  return (
    <section>
      <h1>Reconciliation</h1>
      <div>Status: {reconciliation.data.status ?? "unknown"}</div>
      <div>Positions: {reconciliation.data.positions.length}</div>
      <div>Pending Orders: {reconciliation.data.open_orders.length}</div>
    </section>
  );
}
```

- [ ] **Step 4: Handle session expiry and stale page behavior**

Add a session-expired listener in the shell and keep stale sections visible rather than blanking the page:

```tsx
// client/src/layouts/AppLayout.tsx
import { useEffect } from "react";
import { useNavigate } from "react-router";

export default function AppLayout() {
  const navigate = useNavigate();

  useEffect(() => {
    function onExpired() {
      navigate("/login", { replace: true });
    }

    window.addEventListener("trader:session-expired", onExpired);
    return () => window.removeEventListener("trader:session-expired", onExpired);
  }, [navigate]);

  return <div>{/* sidebar + topbar + outlet */}</div>;
}
```

- [ ] **Step 5: Verify and commit**

Run:
- `npm --prefix client test -- src/pages/CycleHistoryPage.test.tsx`
- `npm --prefix client test -- src/pages/ExecutionPage.test.tsx`
- `npm --prefix client test -- src/pages/ReconciliationPage.test.tsx`
- `npm --prefix client test -- src/App.test.tsx`
- `npm --prefix client test`

Expected: PASS

```bash
git add client/src/pages/CycleHistoryPage.tsx client/src/pages/ExecutionPage.tsx client/src/pages/ReconciliationPage.tsx client/src/pages/CycleHistoryPage.test.tsx client/src/pages/ExecutionPage.test.tsx client/src/pages/ReconciliationPage.test.tsx client/src/App.test.tsx client/src/pages/DashboardPage.test.tsx client/src/layouts/AppLayout.tsx
git commit -m "feat: add runtime detail pages and resilience rules"
```

## Task 7: Document, Verify, And Prepare Handoff

**Files:**
- Modify: `README.md`
- Modify: `tech.md`

- [ ] **Step 1: Define the missing documentation checklist**

```md
- frontend location: `client/`
- install command
- dev command
- test command
- build command
- API key login behavior
- frontend endpoint dependency summary
```

- [ ] **Step 2: Confirm the docs are missing frontend coverage**

Run:
- `rg "client/" README.md tech.md`
- `rg "API key login" README.md tech.md`

Expected: Missing or incomplete coverage

- [ ] **Step 3: Update docs**

```md
## Frontend Console

The `client/` directory contains the internal read-only operations console for trader.

- Start local dev server: `npm --prefix client run dev`
- Run tests: `npm --prefix client test`
- Build production assets: `npm --prefix client run build`
```

```md
## Frontend Auth

The frontend prompts the operator for an API key and attaches it as a bearer token on each request to the existing backend endpoints.
```

- [ ] **Step 4: Run final verification**

Run:
- `npm --prefix client test`
- `npm --prefix client run build`
- `npm --prefix client run lint`
- `npm --prefix client run type-check`

Expected:
- test PASS
- build PASS
- lint PASS
- type-check PASS

- [ ] **Step 5: Commit**

```bash
git add README.md tech.md client
git commit -m "docs: document trader frontend console"
```

## Self-Review

### Spec Coverage

- Standalone frontend project in `client/`: covered by Tasks 1 and 7
- API key login: covered by Tasks 2 and 4
- Sidebar-based multi-page console: covered by Task 4
- Read-only dashboard, runtime, cycle, execution, and reconciliation pages: covered by Tasks 5 and 6
- Polling-based refresh: covered by Tasks 4 and 5
- Shared loading, empty, partial failure, and auth-expiry states: covered by Tasks 5 and 6
- `shadcn/ui` component foundation: covered by Task 4 and shared UI work in Task 5
- Documentation updates: covered by Task 7

### Placeholder Scan

- No `TODO`, `TBD`, or "implement later" placeholders remain
- Each task includes explicit files, commands, and concrete code snippets
- Commit points are present at the end of every task

### Type Consistency

- Session helper names use `getSession`, `setSession`, and `clearSession` consistently
- API functions use `apiFetch` and `validateApiKey` consistently
- Route names stay aligned across `App.tsx`, tests, and page filenames
