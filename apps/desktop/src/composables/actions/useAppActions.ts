import { useI18n } from "vue-i18n";
import { useActivityLog } from "../useActivityLog";
import { useNavigation, type NavigationId } from "../useNavigation";
import { useDocuments } from "../useDocuments";
import { useDesktopState } from "../useDesktopState";
import { confirmDialog, useVault, type ResetStage, type ResetBackend } from "../useVault";
import { useTheme } from "../../theme";
import { useFlash } from "../useFlash";

/*
 * App-level actions that are not about a specific document or version:
 * navigation, theme toggle, manual refresh, and the dev-only stage reset.
 */

export function useAppActions() {
  const { t } = useI18n();
  const { log } = useActivityLog();
  const { setSection, openSettingsTab } = useNavigation();
  const { toggleTheme, isDark } = useTheme();
  const { selectedDocument } = useDocuments();
  const { loadDocuments, resetToStage } = useVault();
  const desktop = useDesktopState();
  const { flash } = useFlash();

  /**
   * Full manual refresh for the context-menu "刷新" entry: reloads the document
   * list (versions included) and re-probes tracked source files. Mirrors the
   * runAction refresh log line. No-op outside Tauri (both underlying calls are).
   */
  async function refreshAll() {
    flash(); // brief full-surface fade so the refresh is visibly "felt"
    const name = selectedDocument.value
      ? selectedDocument.value.name
      : t("log.noDocument");
    log(
      t("log.actionRequested", {
        action: t("actionLogs.refresh"),
        name,
        version: t("log.latest"),
      }),
    );
    await Promise.all([loadDocuments(), desktop.refreshModifications()]);
  }

  /**
   * Reset the isolated test vault to a dev stage. "fresh" wipes it and returns
   * to onboarding; "initial" re-initializes an empty vault with `backend`;
   * "seeded" also imports the sample docs. Dev/test only. Confirms first
   * (destructive) and reloads desktop state so tags/tracked refresh immediately.
   * No-op outside Tauri.
   */
  async function resetToStageAction(
    stage: ResetStage,
    backend: ResetBackend,
    resticPassword?: string,
  ): Promise<void> {
    const stageLabel = t("dev.stageLabel", { n: stageNumber(stage) });
    const actionKey = t("actionLogs.resetToStage", { stage: stageLabel });
    log(
      t("log.actionRequested", {
        action: actionKey,
        name: t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!(await confirmDialog(t(`dev.stages.${stage}.confirm`)))) {
      log(t("log.actionCancelled", { action: actionKey }));
      return;
    }
    void runStageReset(stage, backend, resticPassword, actionKey);
  }

  function stageNumber(stage: ResetStage): number {
    return stage === "fresh" ? 1 : stage === "initial" ? 2 : 3;
  }

  async function runStageReset(
    stage: ResetStage,
    backend: ResetBackend,
    resticPassword: string | undefined,
    actionKey: string,
  ): Promise<void> {
    try {
      await resetToStage(stage, backend, resticPassword);
      await desktop.loadDesktopState();
      log(
        t("dev.resetDone", {
          stage: t("dev.stageLabel", { n: stageNumber(stage) }),
        }),
      );
    } catch (e) {
      log(t("log.actionFailed", { action: actionKey, error: String(e) }));
    }
  }

  function navigate(sectionId: NavigationId) {
    setSection(sectionId);
    const labelKey = `nav.${sectionId}`;
    log(t("actionLogs.navigate", { section: t(labelKey) }));
  }

  /** Open Settings on the 状态 (status) tab - the unified tasks/archive view. */
  function openStatus() {
    openSettingsTab("status");
    log(t("actionLogs.navigate", { section: t("settings.tabs.status") }));
  }

  function toggleCurrentTheme() {
    toggleTheme();
    log(
      t("actionLogs.toggleTheme", {
        theme: t(isDark.value ? "actions.themeDark" : "actions.themeLight"),
      }),
    );
  }

  return {
    navigate,
    openStatus,
    toggleCurrentTheme,
    refreshAll,
    resetToStageAction,
  };
}
