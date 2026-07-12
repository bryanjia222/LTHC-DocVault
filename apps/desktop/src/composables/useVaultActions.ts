import { open, save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "./useActivityLog";
import { useNavigation, type NavigationId } from "./useNavigation";
import { useDocuments } from "./useDocuments";
import { useVault } from "./useVault";
import { useTheme } from "../theme";

/*
 * Centralized action handlers. Every UI action (commit, export, checkout,
 * refresh, navigate, toggle theme) flows through here so the activity log
 * records a consistent message. Commit / export / checkout open a native file
 * dialog, spawn a backend job, and log the job id; the job's truthful state
 * arrives later via `job:update` events (mirrored in useVault).
 */

const OFFICE_EXTENSIONS = ["docx", "xlsx", "pptx"] as const;

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

  function runAction(actionKey: string) {
    if (actionKey === "actionLogs.commit") {
      void commitAction();
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

  async function commitAction() {
    const doc = selectedDocument.value;
    log(
      t("log.actionRequested", {
        action: t("actionLogs.commit"),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!isTauri()) return;
    const path = await pickOfficeFile();
    if (!path) {
      log(t("log.actionCancelled", { action: t("actionLogs.commit") }));
      return;
    }
    try {
      // Commit to the selected document, or create a new one named from the
      // file stem when nothing is selected (first-commit onboarding flow).
      const id = doc
        ? await commit({ path, document_id: doc.id })
        : await commit({ path, new_name: deriveNameFromPath(path) });
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
    await exportOrCheckout("actionLogs.export", "exportVersion");
  }

  async function checkoutAction() {
    await exportOrCheckout("actionLogs.checkout", "checkoutVersion");
  }

  /**
   * Shared flow for export and checkout: both need a selected document +
   * version and a save location. Export writes the file; checkout also marks
   * the version as current (the backend handles both; the UI refreshes on the
   * resulting `job:update`).
   */
  async function exportOrCheckout(
    actionKey: "actionLogs.export" | "actionLogs.checkout",
    fn: "exportVersion" | "checkoutVersion",
  ) {
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
      const params = { document_id: doc.id, version: ver.label, output_path: out };
      const id =
        fn === "exportVersion"
          ? await exportVersion(params)
          : await checkoutVersion(params);
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

async function pickOfficeFile(): Promise<string | null> {
  const result = await open({
    multiple: false,
    filters: [{ name: "Office", extensions: [...OFFICE_EXTENSIONS] }],
  });
  if (!result) return null;
  return Array.isArray(result) ? (result[0] ?? null) : result;
}

function deriveNameFromPath(path: string): string {
  const file = path.replace(/\\/g, "/").split("/").pop() ?? path;
  return file.replace(/\.[^.]+$/, "");
}

function extOf(filename: string): string | null {
  const dot = filename.lastIndexOf(".");
  return dot >= 0 ? filename.slice(dot + 1).toLowerCase() : null;
}
