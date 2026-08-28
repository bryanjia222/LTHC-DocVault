import { ref, watch } from "vue";

/*
 * Sidebar "常用链接" (quick links): user-pinned web bookmarks, persisted in
 * localStorage. A global client UI pref (like theme & dev mode) - not vault
 * data, so no backend state. Each link carries the auto-fetched page title and
 * a favicon as a data URL (the backend fetch_url_meta returns nulls on failure,
 * in which case the UI falls back to a generic link icon + the raw URL).
 */

export interface QuickLink {
  id: string;
  title: string;
  url: string;
  favicon?: string;
}

const STORAGE_KEY = "docvault.quickLinks";

function isQuickLink(value: unknown): value is QuickLink {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v.id === "string" &&
    typeof v.title === "string" &&
    typeof v.url === "string"
  );
}

function readInitial(): QuickLink[] {
  if (typeof localStorage !== "undefined") {
    try {
      const raw = localStorage.getItem(STORAGE_KEY);
      if (!raw) return [];
      const parsed: unknown = JSON.parse(raw);
      if (Array.isArray(parsed)) {
        return parsed.filter(isQuickLink).map((link) => ({
          ...link,
          favicon: typeof link.favicon === "string" ? link.favicon : undefined,
        }));
      }
    } catch {
      // Corrupt / unreadable stored value - start fresh.
    }
  }
  return [];
}

const quickLinks = ref<QuickLink[]>(readInitial());

watch(
  quickLinks,
  (value) => {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
    }
  },
  { deep: true },
);

/** Stable id for a new link. Mirrors useDesktopState's project id helper. */
function makeLinkId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `link-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`;
}

export function useQuickLinks() {
  function addQuickLink(input: {
    title: string;
    url: string;
    favicon?: string;
  }) {
    const id = makeLinkId();
    quickLinks.value = [
      ...quickLinks.value,
      { id, title: input.title, url: input.url, favicon: input.favicon },
    ];
    return id;
  }

  function updateQuickLink(
    id: string,
    patch: Partial<Pick<QuickLink, "title" | "url" | "favicon">>,
  ) {
    quickLinks.value = quickLinks.value.map((link) =>
      link.id === id ? { ...link, ...patch } : link,
    );
  }

  function removeQuickLink(id: string) {
    quickLinks.value = quickLinks.value.filter((link) => link.id !== id);
  }

  return { quickLinks, addQuickLink, updateQuickLink, removeQuickLink };
}
