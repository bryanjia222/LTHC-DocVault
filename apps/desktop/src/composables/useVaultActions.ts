import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "./useActivityLog";
import { useDialogs } from "./useDialogs";
import { useNavigation, type NavigationId } from "./useNavigation";
import { useDocuments } from "./useDocuments";
import { useDesktopState } from "./useDesktopState";
import { useVault, type ResetStage, type ResetBackend } from "./useVault";
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
    resetToStage,
    isTauri,
    libraryPath,
    openLibraryCopy,
    removeLibraryCopy,
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
    if (actionKey === "actionLogs.open") {
      void openDocument();
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
      // Phase A runs synchronously inside commit(): the version is written
      // (pending) and the library copy materialized from its intake before this
      // resolves, so reload the list and baseline the working copy immediately
      // - the user sees "committed" at once. The returned id is the Phase B
      // archive job (compress), which runs on and surfaces in the task bubble;
      // its terminal refreshes the document list + repo size via subscribeJobs.
      await commit({ path, document_id: doc.id });
      await loadDocuments();
      const libPath = await libraryPath({ document_id: doc.id });
      const baseline = await desktop.probeAndBaseline(doc.id, libPath);
      desktop.setTracked(baseline);
      log(t("log.commitSucceeded", { target: doc.name }));
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
      // Checkout switches the current version pointer AND overwrites the
      // library copy with that version's content (output_path). Register a
      // pending track so App.vue refreshes the baseline (the library copy was
      // just rewritten, so status returns to "unchanged").
      const libPath = await libraryPath({ document_id: doc.id });
      const id = await checkoutVersion({
        document_id: doc.id,
        version: ver.label,
        output_path: libPath,
      });
      desktop.registerPendingTrack(id, { kind: "known", docId: doc.id, path: libPath });
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
      // Best-effort: remove the tool-owned library working copy so it does not
      // outlive its document. Failure is non-fatal - the doc is already being
      // unmanaged and the copy can be rebuilt from the archive if needed.
      try {
        await removeLibraryCopy({ document_id: doc.id });
      } catch (e) {
        console.warn("removeLibraryCopy failed", e);
      }
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
      // Phase A is synchronous: the library copy (which is the source here) is
      // already current, so just reload the list and re-baseline it back to
      // "unchanged" immediately. The returned id is the Phase B archive job,
      // which surfaces in the task bubble.
      await commit({
        path,
        document_id: docId,
        note: note || undefined,
      });
      await loadDocuments();
      const libPath = await libraryPath({ document_id: docId });
      const baseline = await desktop.probeAndBaseline(docId, libPath);
      desktop.setTracked(baseline);
      log(t("log.commitSucceeded", { target: doc?.name ?? docId }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /**
   * Open the document's library copy (the tool-owned current-version working
   * copy) in the OS default editor. The backend materializes the copy from the
   * current version first if it is missing (the automated replacement for
   * relink). `docId` defaults to the selected document so the command palette
   * can invoke it without an explicit id. Synchronous - resolves once the
   * editor is launched; editing happens out-of-band and is picked up by
   * modification detection on the next refresh.
   */
  async function openDocument(docId?: string) {
    const actionKey = "actionLogs.open" as const;
    const id = docId ?? selectedDocument.value?.id;
    const doc = documents.value.find((d) => d.id === id);
    // Open the version the user selected - matching what export targets, rather
    // than always the current version. Omit when none is selected, in which case
    // the backend opens the current version's library copy.
    const version = selectedVersion.value?.label;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: version ?? t("log.latest"),
      }),
    );
    if (!id) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    try {
      await openLibraryCopy({ document_id: id, version });
      log(t("log.opened", { name: doc?.name ?? t("log.noDocument") }));
    } catch (e) {
      log(t("log.openFailed", { name: doc?.name ?? t("log.noDocument"), error: String(e) }));
    }
  }

  /**
   * Reset the isolated test vault to a dev stage. "fresh" wipes it and returns
   * to onboarding; "initial" re-initializes an empty vault with `backend`;
   * "seeded" also imports the sample docs. Dev/test only. Confirms first
   * (destructive) and reloads desktop state so tags/tracked refresh immediately.
   * No-op outside Tauri.
   */
  function resetToStageAction(
    stage: ResetStage,
    backend: ResetBackend,
    resticPassword?: string,
  ): void {
    const stageLabel = t("dev.stageLabel", { n: stageNumber(stage) });
    const actionKey = t("actionLogs.resetToStage", { stage: stageLabel });
    log(
      t("log.actionRequested", {
        action: actionKey,
        name: t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (!window.confirm(t(`dev.stages.${stage}.confirm`))) {
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
      log(t("dev.resetDone", { stage: t("dev.stageLabel", { n: stageNumber(stage) }) }));
    } catch (e) {
      log(t("log.actionFailed", { action: actionKey, error: String(e) }));
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
    openDocument,
    resetToStageAction,
    refreshAll,
    deleteDocument,
    renameDocument,
  };
}
