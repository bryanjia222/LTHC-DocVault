import { useI18n } from "vue-i18n";
import { useActivityLog } from "./useActivityLog";
import { useNavigation, type NavigationId } from "./useNavigation";
import { useDocuments } from "./useDocuments";
import { useTheme } from "../theme";

/*
 * Centralized prototype action handlers. Every UI action (commit, export,
 * checkout, refresh, navigate, toggle theme) flows through here so the activity
 * log records a consistent message. Real Tauri command wiring will replace the
 * log calls later.
 */

export function useVaultActions() {
  const { t } = useI18n();
  const { log } = useActivityLog();
  const { setSection } = useNavigation();
  const { toggleTheme, isDark } = useTheme();
  const { selectedDocument, selectedVersion } = useDocuments();

  function runAction(actionKey: string) {
    const name = selectedDocument.value
      ? t(selectedDocument.value.nameKey)
      : t("log.noDocument");
    const version = selectedVersion.value?.label ?? t("log.latest");

    log(t("log.actionRequested", { action: t(actionKey), name, version }));
  }

  function navigate(sectionId: NavigationId) {
    setSection(sectionId);
    const labelKey = `nav.${sectionId}`;
    log(t("actionLogs.navigate", { section: t(labelKey) }));
  }

  function toggleCurrentTheme() {
    toggleTheme();
    log(
      t("actionLogs.toggleTheme", {
        theme: t(isDark.value ? "actions.themeDark" : "actions.themeLight"),
      }),
    );
  }

  return { runAction, navigate, toggleCurrentTheme };
}
