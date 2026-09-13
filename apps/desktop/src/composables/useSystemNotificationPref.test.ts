import { nextTick } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const STORAGE_KEY = "docvault.systemNotificationsEnabled";

describe("useSystemNotificationPref", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.resetModules();
  });

  it("defaults to enabled", async () => {
    const { useSystemNotificationPref } =
      await import("./useSystemNotificationPref");
    expect(useSystemNotificationPref().systemNotificationsEnabled.value).toBe(
      true,
    );
  });

  it("reads and persists the disabled preference", async () => {
    localStorage.setItem(STORAGE_KEY, "false");
    const { useSystemNotificationPref } =
      await import("./useSystemNotificationPref");
    const { systemNotificationsEnabled, setSystemNotificationsEnabled } =
      useSystemNotificationPref();
    expect(systemNotificationsEnabled.value).toBe(false);

    setSystemNotificationsEnabled(true);
    await nextTick();
    expect(localStorage.getItem(STORAGE_KEY)).toBe("true");
  });
});
