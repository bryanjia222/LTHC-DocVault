import { ref } from "vue";

/*
 * App-wide active view state. Shared so the sidebar, command palette, and any
 * other component can switch views without prop drilling.
 *
 * The sidebar now exposes only two top-level destinations: 文档 (documents) and
 * 设置 (settings, pinned to the bottom). Tasks/archive live inside Settings as a
 * "状态" tab, so `settingsTab` remembers which settings tab is open.
 */

export type NavigationId = "documents" | "settings" | "trash";
export type SettingsTab = "status" | "appearance";

const activeSection = ref<NavigationId>("documents");
// Appearance is the first (default) tab; "status" is opened explicitly by the
// sidebar/command-palette 状态 action.
const settingsTab = ref<SettingsTab>("appearance");

export function useNavigation() {
  function setSection(id: NavigationId) {
    activeSection.value = id;
  }

  /** Switch to Settings and open the requested tab (status/appearance).
   * Used by the sidebar, command palette, and the "状态" quick command. */
  function openSettingsTab(tab: SettingsTab) {
    settingsTab.value = tab;
    activeSection.value = "settings";
  }

  return { activeSection, settingsTab, setSection, openSettingsTab };
}
