import { ref, type ComponentPublicInstance } from "vue";
import { useI18n } from "vue-i18n";

import { useActivityLog } from "./useActivityLog";
import { useDocuments } from "./useDocuments";
import { useDialogs } from "./useDialogs";
import { useDoubleClickPref } from "./useDoubleClickPref";
import { useVaultActions } from "./useVaultActions";
import type { Document, Version } from "../data/mock";

interface DocumentSelectionActions {
  /** Called after the global selection changes from an explicit row choice. */
  onDocumentSelected?: (document: Document) => void;
  /** Open the in-app preview for the active or supplied version. */
  openPreview: (version?: Version | null) => void;
}

export function useDocumentSelection(actions: DocumentSelectionActions) {
  const { t } = useI18n();
  const {
    selectedDocument,
    selectedDocumentId,
    selectedVersion,
    selectedVersionId,
    selectDocument,
    selectVersion,
  } = useDocuments();
  const { log } = useActivityLog();
  const { openCommitModified } = useDialogs();
  const { runAction, openDocument } = useVaultActions();
  const { doubleClickAction } = useDoubleClickPref();

  const docMenuRef = ref<
    (ComponentPublicInstance & { openAt(event: MouseEvent): void }) | null
  >(null);
  const versionMenuRef = ref<
    (ComponentPublicInstance & { openAt(event: MouseEvent): void }) | null
  >(null);

  function chooseDocument(document: Document) {
    selectDocument(document);
    actions.onDocumentSelected?.(document);
    log(t("log.selectedDocument", { name: document.name }));
  }

  function chooseVersion(version: Version) {
    selectVersion(version);
    log(
      t("log.selectedVersion", {
        name: selectedDocument.value?.name ?? t("log.noDocument"),
        version: version.label,
      }),
    );
  }

  function openDocMenu(event: MouseEvent, document: Document) {
    selectDocument(document);
    const current = document.versions.find((item) => item.status === "current");
    if (current) selectVersion(current);
    docMenuRef.value?.openAt(event);
  }

  function onGraphContextMenu(payload: {
    version: Version;
    event: MouseEvent;
  }) {
    selectVersion(payload.version);
    versionMenuRef.value?.openAt(payload.event);
  }

  function onDocMenuPreview() {
    actions.openPreview();
  }

  function onVersionMenuPreview() {
    const version = selectedVersion.value;
    if (version) actions.openPreview(version);
  }

  function onRowOpen(document: Document) {
    selectDocument(document);
    void openDocument(document.id);
  }

  function onRowPreview(document: Document) {
    selectDocument(document);
    actions.openPreview();
  }

  function onRowCommit(document: Document) {
    selectDocument(document);
    openCommitModified();
  }

  function onRowExport(document: Document) {
    selectDocument(document);
    runAction("actionLogs.export");
  }

  function onDocDoubleClick(document: Document) {
    selectDocument(document);
    if (doubleClickAction.value === "open") {
      void openDocument(document.id);
    } else {
      actions.openPreview();
    }
  }

  return {
    selectedDocument,
    selectedDocumentId,
    selectedVersion,
    selectedVersionId,
    selectDocument,
    selectVersion,
    chooseDocument,
    chooseVersion,
    docMenuRef,
    versionMenuRef,
    openDocMenu,
    onGraphContextMenu,
    onDocMenuPreview,
    onVersionMenuPreview,
    onRowOpen,
    onRowPreview,
    onRowCommit,
    onRowExport,
    onDocDoubleClick,
  };
}
