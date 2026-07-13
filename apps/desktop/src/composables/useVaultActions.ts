import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "./useActivityLog";
import { useDialogs } from "./useDialogs";
import { useNavigation, type NavigationId } from "./useNavigation";
import { useDocuments } from "./useDocuments";
import { useVault } from "./useVault";
import { useTheme } from "../theme";
import { extOf, pickOfficeFile } from "../utils/file";

/*
 * Centralized action handlers. Every UI action (commit, export, checkout,
 * refresh, navigate, toggle theme) flows through here so the activity log
 * records a consistent message. Commit and export open a native file dialog,
 * spawn a backend job, and log the job id; checkout switches the current
 * version pointer without a file dialog. The job's truthful state arrives
 * later via `job:update` events (mirrored in useVault).
 */

export function useVaultActions() {
  const { t } = useI18n();
  const { log } = useActivityLog();
  const { setSection } = useNavigation();
  const { toggleTheme, isDark } = useTheme();
  const { selectedDocument, selectedVersion } = useDocuments();
  const {
    commit,
    exportVersion,
    checkoutVersion,
    loadDocuments,
    isTauri,
  } = useVault();
  const { openAddDocument } = useDialogs();

  function runAction(actionKey: string) {
    if (actionKey === "actionLogs.addDocument") {
      openAddDocument();
      return;
    }
    if (actionKey === "actionLogs.commit") {
      void commitVersionAction();
      return;
    }
    if (actionKey === "actionLogs.export") {
      void exportAction();
      return;
    }
    if (actionKey === "actionLogs.checkout") {
      void checkoutAction();
      return;
    }
    const name = selectedDocument.value
      ? selectedDocument.value.name
      : t("log.noDocument");
    const version = selectedVersion.value?.label ?? t("log.latest");
    log(t("log.actionRequested", { action: t(actionKey), name, version }));
    if (actionKey === "actionLogs.refresh") {
      void loadDocuments();
    }
  }

  async function commitVersionAction() {
    const doc = selectedDocument.value;
    log(
      t("log.actionRequested", {
        action: t("actionLogs.commit"),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!doc) {
      log(t("log.noSelection", { action: t("actionLogs.commit") }));
      return;
    }
    if (!isTauri()) return;
    const path = await pickOfficeFile();
    if (!path) {
      log(t("log.actionCancelled", { action: t("actionLogs.commit") }));
      return;
    }
    try {
      // Commit a new version to the selected document.
      const id = await commit({ path, document_id: doc.id });
      log(t("log.jobStarted", { action: t("actionLogs.commit"), id }));
    } catch (e) {
      log(
        t("log.actionFailed", {
          action: t("actionLogs.commit"),
          error: String(e),
        }),
      );
    }
  }

  async function exportAction() {
    const actionKey = "actionLogs.export" as const;
    const doc = selectedDocument.value;
    const ver = selectedVersion.value;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: ver?.label ?? t("log.latest"),
      }),
    );
    if (!doc || !ver) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    const ext = extOf(doc.originalFilename) ?? "docx";
    const out = await save({
      defaultPath: `${doc.name}_${ver.label}.${ext}`,
      filters: [{ name: "Office", extensions: [ext] }],
    });
    if (!out) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      // Export writes the selected version to a file; it does not change which
      // version is current.
      const id = await exportVersion({
        document_id: doc.id,
        version: ver.label,
        output_path: out,
      });
      log(t("log.jobStarted", { action: t(actionKey), id }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  async function checkoutAction() {
    const actionKey = "actionLogs.checkout" as const;
    const doc = selectedDocument.value;
    const ver = selectedVersion.value;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: ver?.label ?? t("log.latest"),
      }),
    );
    if (!doc || !ver) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    try {
      // Checkout switches the current version pointer without writing a file.
      // The backend marks the version current; the UI refreshes on `job:update`.
      const id = await checkoutVersion({
        document_id: doc.id,
        version: ver.label,
      });
      log(t("log.jobStarted", { action: t(actionKey), id }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
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
