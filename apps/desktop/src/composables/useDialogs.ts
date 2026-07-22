import { ref } from "vue";

/*
 * App-wide modal open-state. Module-level singletons so any component (the
 * centralized action handler, a Settings button) can open a dialog that is
 * mounted once at the app root. Each dialog resets its own form fields when it
 * opens, so the open flags carry no payload - just "show this dialog".
 */

const addDocumentOpen = ref(false);
const switchBackendOpen = ref(false);
const commitModifiedOpen = ref(false);
const documentStatusOpen = ref(false);
const renameOpen = ref(false);
const noteEditOpen = ref(false);
const newDocumentOpen = ref(false);
// Which project the "新建文件" kebab action originated from (null = the
// all-documents root - the new doc is created with no project membership).
// Unlike the other open flags (which carry no payload), this one needs a
// target because the new doc's project assignment depends on where the user
// clicked, not on the selected document.
const newDocumentProjectId = ref<string | null>(null);

export function useDialogs() {
  function openAddDocument() {
    addDocumentOpen.value = true;
  }
  function closeAddDocument() {
    addDocumentOpen.value = false;
  }
  function openSwitchBackend() {
    switchBackendOpen.value = true;
  }
  function closeSwitchBackend() {
    switchBackendOpen.value = false;
  }
  function openCommitModified() {
    commitModifiedOpen.value = true;
  }
  function closeCommitModified() {
    commitModifiedOpen.value = false;
  }
  function openDocumentStatus() {
    documentStatusOpen.value = true;
  }
  function closeDocumentStatus() {
    documentStatusOpen.value = false;
  }
  function openRename() {
    renameOpen.value = true;
  }
  function closeRename() {
    renameOpen.value = false;
  }
  function openNoteEdit() {
    noteEditOpen.value = true;
  }
  function closeNoteEdit() {
    noteEditOpen.value = false;
  }
  /** Open the new-document dialog. `projectId` is the project to assign the
   *  new doc to (null / omitted = root, no project membership). */
  function openNewDocument(projectId?: string | null) {
    newDocumentProjectId.value = projectId ?? null;
    newDocumentOpen.value = true;
  }
  function closeNewDocument() {
    newDocumentOpen.value = false;
  }

  return {
    addDocumentOpen,
    switchBackendOpen,
    commitModifiedOpen,
    documentStatusOpen,
    renameOpen,
    noteEditOpen,
    newDocumentOpen,
    newDocumentProjectId,
    openAddDocument,
    closeAddDocument,
    openSwitchBackend,
    closeSwitchBackend,
    openCommitModified,
    closeCommitModified,
    openDocumentStatus,
    closeDocumentStatus,
    openRename,
    closeRename,
    openNoteEdit,
    closeNoteEdit,
    openNewDocument,
    closeNewDocument,
  };
}
