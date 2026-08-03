import { message, save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "../useActivityLog";
import { useDialogs } from "../useDialogs";
import { useDocuments } from "../useDocuments";
import { useDesktopState } from "../useDesktopState";
import { confirmDialog, useVault } from "../useVault";
import {
  deriveNameFromPath,
  extOf,
  pickDocumentFile,
  pickDocumentFiles,
} from "../../utils/file";

/*
 * Document lifecycle + import/export/open actions: pick-first import, batch
 * import, commit (new version / modified / replace), version export, checkout,
 * open-in-editor, rename, and note editing. Also owns `runAction`, the dispatch
 * table that maps command-palette action keys to the handlers defined here.
 */

export function useDocumentActions() {
  const { t } = useI18n();
  const { log } = useActivityLog();
  const {
    selectedDocument,
    selectedVersion,
    documents,
    activeProjectId,
  } = useDocuments();
  const {
    commit,
    exportVersion,
    exportWorkingCopy,
    checkoutVersion,
    renameDocument: sendRename,
    setVersionNote: sendSetVersionNote,
    loadDocuments,
    isTauri,
    libraryPath,
    openLibraryCopy,
  } = useVault();
  const desktop = useDesktopState();
  const { openAddDocument } = useDialogs();

  function runAction(actionKey: string) {
    if (actionKey === "actionLogs.addDocument") {
      void startImport();
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
   * Pick-first import entry point. Opens the native multi-select picker, then
   * hands the chosen files to the add-document dialog. The default import
   * directory is the active sidebar project unless an explicit project is
   * passed (project-kebab menu). No-op outside Tauri.
   */
  async function startImport(projectId?: string | null) {
    if (!isTauri()) return;
    const files = await pickDocumentFiles();
    if (files.length === 0) return; // cancelled
    openAddDocument(files, projectId ?? activeProjectId.value ?? null);
  }

  /**
   * Import a batch of files as new documents. Core, testable loop: one Phase-A
   * commit at a time, reloading + snapshot-diffing to find each created doc (so
   * name collisions never mis-match), baselining its library copy, and
   * assigning it to `projectId` (null = unassigned). Per-file failures are
   * collected and do not abort the rest of the batch. `onProgress` reports the
   * number of files attempted (done + failed) as the loop advances.
   */
  async function importDocuments(
    files: Array<{ path: string; name?: string; author?: string }>,
    projectId: string | null,
    onProgress?: (done: number, total: number) => void,
  ): Promise<{ ok: number; failed: Array<{ path: string; error: string }> }> {
    const failed: Array<{ path: string; error: string }> = [];
    let ok = 0;
    for (const file of files) {
      try {
        // Per-file snapshot: the only doc not in it after THIS commit is the
        // one just imported, so the name-collision fallback stays unambiguous.
        const snapshotIds = documents.value.map((d) => d.id);
        const resolvedName = file.name?.trim() || deriveNameFromPath(file.path);
        await commit({
          path: file.path,
          new_name: resolvedName,
          author: file.author?.trim() || undefined,
        });
        await loadDocuments();
        const created =
          documents.value.find(
            (d) => !snapshotIds.includes(d.id) && d.name === resolvedName,
          ) ?? documents.value.find((d) => !snapshotIds.includes(d.id));
        if (created) {
          const libPath = await libraryPath({ document_id: created.id });
          const baseline = await desktop.probeAndBaseline(created.id, libPath);
          desktop.setTracked(baseline);
          if (projectId) desktop.setDocumentProject(created.id, projectId);
        }
        ok += 1;
      } catch (e) {
        failed.push({ path: file.path, error: String(e) });
      }
      onProgress?.(ok + failed.length, files.length);
    }
    return { ok, failed };
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
    const path = await pickDocumentFile();
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
    // Export the document's working copy - the library file that mirrors the
    // current version and holds the user's uncommitted edits - NOT the last
    // committed version. So an uncommitted document exports its uncommitted
    // state. `performExport` runs the save + file copy.
    await performExport();
  }

  /**
   * Run the actual working-copy export: native save dialog, then the
   * `export_working_copy` file copy. No version is involved - the working copy
   * is a single live file, so the default file name is just the doc name.
   */
  async function performExport() {
    const actionKey = "actionLogs.export" as const;
    const doc = selectedDocument.value;
    if (!doc) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    const ext = extOf(doc.originalFilename) ?? "docx";
    const out = await save({
      defaultPath: `${doc.name}.${ext}`,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (!out) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      await exportWorkingCopy({ document_id: doc.id, output_path: out });
      log(t("log.exported", { target: doc.name }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  /**
   * Export a specific committed version (version-history right-click). Unlike
   * `performExport` (the working copy), this serves the archived snapshot for the
   * given version label via the `export_version` job - so it does not capture
   * uncommitted edits. The default file name includes the version label.
   */
  async function exportVersionAction(versionLabel: string) {
    const actionKey = "actionLogs.export" as const;
    const doc = selectedDocument.value;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: doc?.name ?? t("log.noDocument"),
        version: versionLabel,
      }),
    );
    if (!doc) {
      log(t("log.noSelection", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    const ext = extOf(doc.originalFilename) ?? "docx";
    const out = await save({
      defaultPath: `${doc.name}_${versionLabel}.${ext}`,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (!out) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      const id = await exportVersion({
        document_id: doc.id,
        version: versionLabel,
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
    // Checkout switches the current-version pointer. Switching to the version
    // that is already current is a no-op (and would just rewrite the library
    // copy with identical bytes), so refuse it up front with a clear log line.
    if (ver.status === "current") {
      log(t("log.alreadyCurrent", { name: doc.name, version: ver.label }));
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
      desktop.registerPendingTrack(id, {
        kind: "known",
        docId: doc.id,
        path: libPath,
      });
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
   * Update the selected version's note (its commit message). Called by the
   * note-edit dialog after it collects the new text. A note unchanged from the
   * current value is treated as a cancel (no backend call). An empty note clears
   * it (sent as null). Synchronous, so the document list is reloaded on success
   * and a noteUpdated entry is logged.
   */
  async function editVersionNote(note: string) {
    const actionKey = "actionLogs.editNote" as const;
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
    const trimmed = note.trim();
    if (trimmed === ver.note.trim()) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    if (!isTauri()) return;
    try {
      await sendSetVersionNote({
        document_id: doc.id,
        version_id: ver.id,
        note: trimmed || null,
      });
      await loadDocuments();
      log(t("log.noteUpdated", { name: doc.name, version: ver.label }));
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
   * Replace the document's current file with a user-picked file and commit it
   * as a new version - the library copy (working copy) is materialized from the
   * picked file's intake, so the working copy is effectively replaced. Mirrors
   * `commitVersionAction` (pick file -> `commit({ path, document_id })`), but
   * guards the pending working copy: if the tracker reports "modified", the
   * user's uncommitted changes would be overwritten by the replacement, so we
   * confirm and commit them first (as their own new version) before replacing.
   * `docId` defaults to the selected document so the context menu can invoke it
   * without an explicit id.
   */
  async function replaceCommitDocument(docId?: string) {
    const actionKey = "actionLogs.replaceCommit" as const;
    const id = docId ?? selectedDocument.value?.id;
    const doc = documents.value.find((d) => d.id === id);
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
    // Pick the replacement file FIRST, before any precondition side effect, so a
    // cancel here never disturbs the working copy. The picker is restricted to the
    // document's own extension so the user can only choose a same-type file; the
    // extension check below is the authoritative guard (the OS filter is a
    // convenience and some platforms still expose an "All files" override).
    const expected = extOf(doc.originalFilename);
    const path = await pickDocumentFile(expected);
    if (!path) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    const picked = extOf(path);
    if (expected !== picked) {
      await message(
        t("source.replaceCommitTypeMismatch", {
          name: doc.name,
          expected: expected ?? "",
          picked: picked ?? "",
        }),
        { title: t("source.replaceCommitTypeMismatchTitle"), kind: "error" },
      );
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    // Precondition: don't silently drop uncommitted working-copy changes. If the
    // tracker reports "modified", confirm and commit the current copy first (as
    // a new version), then replace. "missing"/"unchanged"/"none" need no
    // pre-commit - "missing" in particular just means we are relinking the file.
    if (desktop.modificationFor(doc.id) === "modified") {
      const ok = await confirmDialog(
        t("source.replaceCommitConfirm", { name: doc.name }),
      );
      if (!ok) {
        log(t("log.actionCancelled", { action: t(actionKey) }));
        return;
      }
      await commitModifiedDocument(doc.id);
      // commitModifiedDocument swallows its own errors (logs actionFailed,
      // returns normally). If the pre-commit didn't land, abort so the pending
      // changes aren't lost when the replacement file overwrites the working copy.
      if (desktop.modificationFor(doc.id) === "modified") return;
    }
    try {
      // Phase A runs synchronously inside commit(): the version is written
      // (pending) and the library copy materialized from the picked file's
      // intake before this resolves, so reload the list and baseline the working
      // copy immediately - the user sees "committed" at once. The returned id is
      // the Phase B archive job (compress), which runs on and surfaces in the
      // task bubble; its terminal refreshes the document list + repo size via
      // subscribeJobs.
      await commit({ path, document_id: doc.id });
      await loadDocuments();
      const libPath = await libraryPath({ document_id: doc.id });
      const baseline = await desktop.probeAndBaseline(doc.id, libPath);
      desktop.setTracked(baseline);
      log(t("log.commitSucceeded", { target: doc.name }));
    } catch (e) {
      log(
        t("log.actionFailed", {
          action: t(actionKey),
          error: String(e),
        }),
      );
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
      const error = String(e);
      log(
        t("log.openFailed", { name: doc?.name ?? t("log.noDocument"), error }),
      );
      // No default app (or it failed to launch) - surface a visible, actionable
      // prompt; the activity log alone is easy to miss.
      await message(
        t("log.openNoDefaultApp", {
          name: doc?.name ?? t("log.noDocument"),
          error,
        }),
        { title: t("log.openFailedTitle"), kind: "error" },
      );
    }
  }

  return {
    runAction,
    startImport,
    importDocuments,
    commitModifiedDocument,
    replaceCommitDocument,
    openDocument,
    exportVersionAction,
    renameDocument,
    editVersionNote,
  };
}
