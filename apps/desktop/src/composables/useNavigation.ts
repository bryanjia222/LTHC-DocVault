import { ref } from "vue";

/*
 * App-wide active view state. Shared so the sidebar, command palette, and any
 * other component can switch views without prop drilling.
 */

export type NavigationId = "documents" | "jobs" | "archive" | "settings";

export interface NavigationItem {
  id: NavigationId;
  labelKey: string;
}

export const navigationItems: NavigationItem[] = [
  { id: "documents", labelKey: "nav.documents" },
  { id: "jobs", labelKey: "nav.jobs" },
  { id: "archive", labelKey: "nav.archive" },
  { id: "settings", labelKey: "nav.settings" },
];

const activeSection = ref<NavigationId>("documents");

export function useNavigation() {
  function setSection(id: NavigationId) {
    activeSection.value = id;
  }

  return { activeSection, setSection };
}
