import { message } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import { useActivityLog } from "../useActivityLog";
import { useDocuments } from "../useDocuments";
import { useDesktopState } from "../useDesktopState";
import { confirmDialog, useVault } from "../useVault";
import { ancestorsOf, descendantsOf } from "../../utils/versionTree";

/*
 * Recycle-bin actions: soft-delete (desktop-local hide) and the irreversible
 * permanent deletes, for both documents and versions, plus emptying the bin.
 * The backend delete/restore is driven here; bin membership lives in
 * desktop-state.
 */

export function useTrashActions() {
  const { t } = useI18n();
  const { log } = useActivityLog();
  const {
    selectedDocument,
    selectedDocumentId,
    documents,
    selectFirstVisible,
  } = useDocuments();
  const {
    deleteDocument: sendDelete,
    deleteVersions: sendDeleteVersions,
    removeLibraryCopy,
    loadDocuments,
    isTauri,
  } = useVault();
  const desktop = useDesktopState();

  /**
   * Soft-delete the selected document: move it to the recycle bin (a
   * desktop-local hide). The vault still holds the document and all its history;
   * the user can restore it or permanently delete it from the bin. This is the
   * reversible delete from the document list, so a single confirmation suffices
   * - the irreversible "all history deleted" warning + double-confirm lives on
   * `permanentlyDeleteDocument` / `emptyTrash`. If the just-trashed document was
   * the active selection, the detail panel moves to the next visible document.
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
    if (!(await confirmDialog(t("confirm.moveToTrash", { name: doc.name })))) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    desktop.trashDoc(doc.id);
    if (selectedDocumentId.value === doc.id) selectFirstVisible();
    log(t("log.movedToTrash", { target: doc.name }));
  }

  /** Restore a document from the recycle bin (un-hide). No backend call - the
   *  document and its history were never removed, only hidden. */
  function restoreDocument(docId: string) {
    const actionKey = "actionLogs.restore" as const;
    const doc = documents.value.find((d) => d.id === docId);
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
    desktop.restoreDoc(docId);
    log(t("log.restored", { target: doc.name }));
  }

  /**
   * Permanently delete a document from the recycle bin: this is the irreversible
   * step that "unmanages" the document - it removes DB rows, restic snapshots,
   * and the local archive directory (never the user's source file). Double-
   * confirmed because ALL version history and backup snapshots are gone for good.
   * Desktop-local annotations + bin membership are cleared right away; the job's
   * truthful state arrives later via `job:update`.
   */
  async function permanentlyDeleteDocument(docId: string) {
    const actionKey = "actionLogs.delete" as const;
    const doc = documents.value.find((d) => d.id === docId);
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
    if (
      !(await confirmDialog(t("confirm.permanentDelete", { name: doc.name })))
    ) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    if (
      !(await confirmDialog(
        t("confirm.permanentDeleteAgain", { name: doc.name }),
      ))
    ) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      const id = await sendDelete({ document_id: doc.id });
      desktop.clearDoc(doc.id);
      // Best-effort: remove the tool-owned library working copy so it does not
      // outlive its document. Failure is non-fatal.
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
   * Empty the recycle bin: permanently delete every document AND every version
   * in it. Like `permanentlyDeleteDocument` this is irreversible (all history +
   * snapshots removed), so it is double-confirmed. Each document is unmanaged in
   * turn; a per-document failure is logged but does not abort the rest of the
   * batch. Trashed versions are batched per document into one `delete_versions`
   * job each; versions whose document is itself in the bin are skipped here -
   * the document delete removes all its versions anyway, so a second delete would
   * race / fail on a missing document.
   */
  async function emptyTrash() {
    const actionKey = "actionLogs.emptyTrash" as const;
    const docIds = desktop.trashedIds();
    const trashedDocSet = new Set(docIds);
    // Only versions of still-live documents need their own delete; versions of
    // trashed documents are removed by the document's delete_document job.
    const versionsByDoc = new Map<string, string[]>();
    for (const entry of desktop.trashedVersionList()) {
      if (trashedDocSet.has(entry.documentId)) continue;
      const list = versionsByDoc.get(entry.documentId);
      if (list) list.push(entry.versionId);
      else versionsByDoc.set(entry.documentId, [entry.versionId]);
    }
    const versionCount = [...versionsByDoc.values()].reduce(
      (n, v) => n + v.length,
      0,
    );
    const total = docIds.length + versionCount;
    log(
      t("log.actionRequested", {
        action: t(actionKey),
        name: t("log.noDocument"),
        version: t("log.latest"),
      }),
    );
    if (total === 0) {
      log(t("log.trashEmpty"));
      return;
    }
    if (!isTauri()) return;
    if (!(await confirmDialog(t("confirm.emptyTrash", { count: total })))) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    if (
      !(await confirmDialog(t("confirm.emptyTrashAgain", { count: total })))
    ) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    for (const id of docIds) {
      try {
        await sendDelete({ document_id: id });
        desktop.clearDoc(id);
        try {
          await removeLibraryCopy({ document_id: id });
        } catch (e) {
          console.warn("removeLibraryCopy failed", e);
        }
      } catch (e) {
        const doc = documents.value.find((d) => d.id === id);
        log(
          t("log.actionFailed", {
            action: t(actionKey),
            error: `${doc?.name ?? id}: ${String(e)}`,
          }),
        );
      }
    }
    for (const [docId, versionIds] of versionsByDoc) {
      try {
        await sendDeleteVersions({
          document_id: docId,
          version_ids: versionIds,
        });
        for (const versionId of versionIds) {
          desktop.clearVersion(docId, versionId);
        }
      } catch (e) {
        const doc = documents.value.find((d) => d.id === docId);
        log(
          t("log.actionFailed", {
            action: t(actionKey),
            error: `${doc?.name ?? docId}: ${String(e)}`,
          }),
        );
      }
    }
    log(t("log.trashEmptied", { count: total }));
  }

  /**
   * Soft-delete a single version to the recycle bin: a desktop-local hide. The
   * vault still holds the version (and the rest of the history) until it is
   * permanently deleted from the bin. Because deleting a version orphans its
   * children, a version is only ever trashed TOGETHER with all of its
   * descendants - so when this version has descendants, the confirm dialog
   * lists them and asks whether to delete them too; "No" cancels the entire
   * delete (the version is never orphaned from its children). The current
   * version and the document's whole history are never deletable this way
   * (switch to another version first / delete the document instead).
   */
  async function deleteVersion(docId: string, versionId: string) {
    const actionKey = "actionLogs.deleteVersion" as const;
    const doc = documents.value.find((d) => d.id === docId);
    const ver = doc?.versions.find((v) => v.id === versionId);
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
    // Subtree-together: this version plus every descendant moves as one unit.
    const descendants = descendantsOf(doc.versions, versionId);
    const subtreeIds = [versionId, ...descendants.map((d) => d.id)];
    // The current version is never deletable (it is the checked-out basis). This
    // covers the version itself AND any descendant that happens to be current -
    // deleting an ancestor would trash the current version too.
    const current = doc.versions.find((v) => v.status === "current");
    if (current && subtreeIds.includes(current.id)) {
      await message(t("versionMenu.deleteBlockedCurrent"), {
        title: t("versionMenu.deleteBlockedTitle"),
        kind: "warning",
      });
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    // Refuse to empty the document via version delete - if the subtree is the
    // whole history, delete the document instead.
    if (subtreeIds.length >= doc.versions.length) {
      await message(t("versionMenu.deleteBlockedLast"), {
        title: t("versionMenu.deleteBlockedTitle"),
        kind: "warning",
      });
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    let confirmed: boolean;
    if (descendants.length > 0) {
      confirmed = await confirmDialog(
        t("confirm.deleteVersionDescendants", {
          name: doc.name,
          version: ver.label,
          descendants: descendants.map((d) => d.label).join(", "),
        }),
      );
    } else {
      confirmed = await confirmDialog(
        t("confirm.deleteVersion", { name: doc.name, version: ver.label }),
      );
    }
    if (!confirmed) {
      // "No" cancels the entire delete - the version is not orphaned from its
      // children, and nothing moves to the bin.
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    // Trash the whole subtree as flat per-version entries (so each can be
    // restored / permanently deleted individually from the bin).
    for (const id of subtreeIds) desktop.trashVersion(docId, id);
    log(
      t("log.versionMovedToTrash", {
        name: doc.name,
        version: ver.label,
        count: subtreeIds.length,
      }),
    );
  }

  /**
   * Restore a single version from the recycle bin (un-hide). No backend call -
   * the version was only hidden, never removed. Symmetric with delete: just as a
   * delete cascades DOWN to descendants (so a version is never orphaned from its
   * children), a restore cascades UP to ancestors. Restoring a version while one
   * of its ancestors is still in the bin would re-expose it with a hidden parent
   * (orphaning it - in the list it shows as a detached root, in the tree it does
   * not render at all), so when any trashed ancestor exists the user is asked
   * whether to restore those ancestors too; "No" cancels the entire restore (the
   * version stays trashed, never orphaned), mirroring the delete-side "No cancels
   * entirely".
   */
  async function restoreVersion(docId: string, versionId: string) {
    const actionKey = "actionLogs.restoreVersion" as const;
    const doc = documents.value.find((d) => d.id === docId);
    const ver = doc?.versions.find((v) => v.id === versionId);
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
    // Ancestors still in the bin: restoring without them would orphan this
    // version (its parent hidden), so they must come back together. Nearest-first
    // so the confirm lists "v2, v1" in reading order.
    const trashedAncestors = ancestorsOf(doc.versions, versionId).filter((a) =>
      desktop.isVersionTrashed(docId, a.id),
    );
    if (trashedAncestors.length > 0) {
      const confirmed = await confirmDialog(
        t("confirm.restoreVersionAncestors", {
          name: doc.name,
          version: ver.label,
          ancestors: trashedAncestors.map((a) => a.label).join(", "),
        }),
      );
      if (!confirmed) {
        // "No" cancels the entire restore - the version stays trashed rather than
        // being re-exposed with a hidden parent (never orphaned).
        log(t("log.actionCancelled", { action: t(actionKey) }));
        return;
      }
      for (const ancestor of trashedAncestors) {
        desktop.restoreVersion(docId, ancestor.id);
      }
    }
    desktop.restoreVersion(docId, versionId);
    log(
      trashedAncestors.length > 0
        ? t("log.versionRestoredWithAncestors", {
            name: doc.name,
            version: ver.label,
            ancestors: trashedAncestors.map((a) => a.label).join(", "),
          })
        : t("log.versionRestored", { name: doc.name, version: ver.label }),
    );
  }

  /**
   * Permanently delete a version (and the descendants that were trashed with
   * it) from the recycle bin: this is the irreversible step that removes the DB
   * row(s), forgets restic snapshots, and deletes the local archive directory.
   * Double-confirmed. Only trashed versions are removed - a descendant that was
   * restored (still live) is left in place, so a visible version is never
   * deleted by surprise (it may end up with a dangling parent reference, which
   * the tree view surfaces cosmetically). Desktop bin membership is cleared
   * once the job reports success; the truthful state arrives later via
   * `job:update`.
   */
  async function permanentlyDeleteVersion(docId: string, versionId: string) {
    const actionKey = "actionLogs.deleteVersion" as const;
    const doc = documents.value.find((d) => d.id === docId);
    const ver = doc?.versions.find((v) => v.id === versionId);
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
    // Remove this version plus any of its descendants that are still in the bin
    // (they were trashed together). Non-trashed descendants are left alive.
    const descendants = descendantsOf(doc.versions, versionId);
    const toDelete = [
      versionId,
      ...descendants
        .filter((d) => desktop.isVersionTrashed(docId, d.id))
        .map((d) => d.id),
    ];
    const descendantLabels = descendants
      .filter((d) => desktop.isVersionTrashed(docId, d.id))
      .map((d) => d.label);
    if (descendantLabels.length > 0) {
      if (
        !(await confirmDialog(
          t("confirm.permanentDeleteVersionDescendants", {
            name: doc.name,
            version: ver.label,
            descendants: descendantLabels.join(", "),
          }),
        ))
      ) {
        log(t("log.actionCancelled", { action: t(actionKey) }));
        return;
      }
    } else {
      if (
        !(await confirmDialog(
          t("confirm.permanentDeleteVersion", {
            name: doc.name,
            version: ver.label,
          }),
        ))
      ) {
        log(t("log.actionCancelled", { action: t(actionKey) }));
        return;
      }
    }
    if (
      !(await confirmDialog(
        t("confirm.permanentDeleteVersionAgain", {
          name: doc.name,
          version: ver.label,
        }),
      ))
    ) {
      log(t("log.actionCancelled", { action: t(actionKey) }));
      return;
    }
    try {
      const id = await sendDeleteVersions({
        document_id: doc.id,
        version_ids: toDelete,
      });
      for (const vid of toDelete) desktop.clearVersion(doc.id, vid);
      await loadDocuments();
      log(t("log.jobStarted", { action: t(actionKey), id }));
    } catch (e) {
      log(t("log.actionFailed", { action: t(actionKey), error: String(e) }));
    }
  }

  return {
    deleteDocument,
    restoreDocument,
    permanentlyDeleteDocument,
    emptyTrash,
    deleteVersion,
    restoreVersion,
    permanentlyDeleteVersion,
  };
}
