import { vi } from "vitest";

/*
 * Global test setup: stub the Tauri IPC boundary so no test ever depends on a
 * real Tauri runtime. Pure-helper tests do not touch these; composable and
 * component tests can override resolved values per test with
 * `vi.mocked(invoke).mockResolvedValue(...)` (or `mockResolvedValueOnce`).
 *
 * `listen` resolves to an unlisten function, matching Tauri's signature.
 */
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => undefined),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));
