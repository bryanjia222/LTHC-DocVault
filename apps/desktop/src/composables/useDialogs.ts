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
/** Export-commit prompt: shown when exporting a doc with uncommitted edits, so
 *  the user can commit first (exports only commit the last committed version) or
 *  export the committed version directly. */
const exportCommitPromptOpen = ref(false);

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
  function openExportCommitPrompt() {
    exportCommitPromptOpen.value = true;
  }
  function closeExportCommitPrompt() {
    exportCommitPromptOpen.value = false;
  }

  return {
    addDocumentOpen,
    switchBackendOpen,
    commitModifiedOpen,
    documentStatusOpen,
    renameOpen,
    exportCommitPromptOpen,
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
    openExportCommitPrompt,
    closeExportCommitPrompt,
  };
}
