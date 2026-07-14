import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "./useActivityLog";
import { useDialogs } from "./useDialogs";
import { useNavigation, type NavigationId } from "./useNavigation";
import { useDocuments } from "./useDocuments";
import { useDesktopState } from "./useDesktopState";
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
  const { selectedDocument, selectedVersion, documents } = useDocuments();
  const {
    commit,
    exportVersion,
    checkoutVersion,
    deleteDocument: sendDelete,
    renameDocument: sendRename,
    loadDocuments,
    resetVault,
    seedDemoDocs,
    isTauri,
  } = useVault();
  const desktop = useDesktopState();
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

  /**
   * Full manual refresh for the context-menu "刷新" entry: reloads the document
   * list (versions included) and re-probes tracked source files. Mirrors the
   * runAction refresh log line. No-op outside Tauri (both underlying calls are).
   */
  async function refreshAll() {
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
      // Commit a new version to the selected document. Register a pending track
      // so App.vue captures the picked file as the document's tracked source
      // (fresh baseline) once the commit job succeeds.
      const id = await commit({ path, document_id: doc.id });
      desktop.registerPendingTrack(id, { kind: "known", docId: doc.id, path });
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

  /**
   * Delete the selected document: confirm (destructive), then spawn the backend
   * delete job. Delete only "unmanages" the document - it removes DB rows, restic
   * snapshots, and the local archive directory, but never the user's source
   * file. Desktop-local annotations (tags / tracked source) are cleared right
   * away so no orphaned metadata lingers; the document list refreshes when the
   * job succeeds (refreshKinds includes "delete"). The job's truthful state
   * arrives later via `job:update`.
   */
  async function deleteDocument() {
    const actionKey = "actionLogs.delete" as const;
    const doc = selectedDocument.value;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!doc) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    if (!window.confirm(t("confirm.delete", { name: doc.name }))) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      const id = await sendDelete({ document_id: doc.id });
      desktop.clearDoc(doc.id);
      log(t("log.jobStarted", { action: t(actionKey), id }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /**
   * Rename the selected document (DB name only; versions are untouched). Called
   * by the rename dialog after it collects the new name. A blank or unchanged
   * name is treated as a cancel. Synchronous, so the document list is reloaded
   * on success and a renamed entry is logged.
   */
  async function renameDocument(newName: string) {
    const actionKey = "actionLogs.rename" as const;
    const doc = selectedDocument.value;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!doc) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    const trimmed = newName.trim();
    if (!trimmed || trimmed === doc.name) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    try {
      await sendRename({ document_id: doc.id, new_name: trimmed });
      await loadDocuments();
      log(t("log.renamed", { name: doc.name, newName: trimmed }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /**
   * Commit the document's tracked source file as a new version directly - no
   * file dialog, since the path is already known. Only meaningful when the
   * tracker reports "modified"; the UI disables the button otherwise. Registers
   * a pending track so App.vue refreshes the baseline (back to "unchanged") once
   * the commit succeeds. `note` is the optional commit message collected by the
   * commit-modified dialog; omitted (undefined) when blank.
   */
  async function commitModifiedDocument(docId: string, note?: string) {
    const actionKey = "actionLogs.commitModified" as const;
    const doc = documents.value.find((d) => d.id === docId);
    const path = desktop.trackedPathFor(docId);
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!doc) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!path) {
      log(t("log.noTrackedFile", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    try {
      const id = await commit({
        path,
        document_id: docId,
        note: note || undefined,
      });
      desktop.registerPendingTrack(id, { kind: "known", docId, path });
      log(t("log.jobStarted", { action: t(actionKey), id }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /**
   * Re-specify a document's tracked source file: pick a working copy, probe it
   * for a fresh baseline, and record it. The recovery path for a missing source
   * (deleted/moved/changed machines) and for documents not yet tracked on this
   * machine. Status returns to "unchanged" until the file is edited again.
   */
  async function relinkSourceFile(docId: string) {
    const actionKey = "actionLogs.relinkSource" as const;
    const doc = documents.value.find((d) => d.id === docId);
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!doc) return;
    if (!isTauri()) return;
    const path = await pickOfficeFile();
    if (!path) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      const baseline = await desktop.probeAndBaseline(docId, path);
      desktop.setTracked(baseline);
      log(t("log.relinked", { name: doc.name, path }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /** Stop tracking a document's source file (removes the baseline). */
  function stopTracking(docId: string) {
    const doc = documents.value.find((d) => d.id === docId);
    desktop.clearTracked(docId);
    log(t("log.stoppedTracking", { name: doc?.name ?? t("log.noDocument") }));
  }

  /**
   * Reset the desktop to a known state for testing: "empty" wipes the isolated
   * test vault; "seeded" also imports three sample docs with tags + source
   * baselines. Dev/test only. Confirms first (destructive) and reloads desktop
   * state so tags/tracked refresh immediately. No-op outside Tauri.
   */
  function resetVaultAction(mode: "empty" | "seeded"): void {
    const actionKey =
      mode === "seeded" ? "actionLogs.seedDemo" : "actionLogs.resetVault";
    const confirmKey =
      mode === "seeded" ? "dev.confirmSeeded" : "dev.confirmEmpty";
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!window.confirm(t(confirmKey))) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    void runReset(mode, actionKey);
  }

  async function runReset(
    mode: "empty" | "seeded",
    actionKey: string,
  ): Promise<void> {
    try {
      if (mode === "seeded") await seedDemoDocs();
      else await resetVault();
      await desktop.loadDesktopState();
      log(mode === "seeded" ? t("log.seeded", { count: 3 }) : t("dev.resetDone"));
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

  return {
    runAction,
    navigate,
    toggleCurrentTheme,
    commitModifiedDocument,
    relinkSourceFile,
    stopTracking,
    resetVaultAction,
    refreshAll,
    deleteDocument,
    renameDocument,
  };
}
