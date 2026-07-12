import { computed, ref } from "vue";
import { useVault } from "./useVault";
import type { Document, Version } from "../data/mock";

/*
 * Document selection + search state. Shared app-wide so the topbar actions and
 * command palette can reference the currently selected document/version. The
 * document list itself is owned by useVault (backed by the real Tauri commands,
 * or mock fixtures in browser dev); this composable layers selection + filtering
 * on top of it.
 */

const { documents } = useVault();
const selectedDocumentId = ref<string>(documents.value[0]?.id ?? "");
const selectedVersionId = ref<string>(
  documents.value[0]?.versions[0]?.id ?? "",
);
const searchQuery = ref("");

const selectedDocument = computed<Document | undefined>(
  () =>
    documents.value.find(
      (document) => document.id === selectedDocumentId.value,
    ) ?? documents.value[0],
);

const selectedVersion = computed<Version | undefined>(
  () =>
    selectedDocument.value?.versions.find(
      (version) => version.id === selectedVersionId.value,
    ) ?? selectedDocument.value?.versions[0],
);

export function useDocuments() {
  const filteredDocuments = computed<Document[]>(() => {
    const query = searchQuery.value.trim().toLowerCase();

    if (!query) {
      return documents.value;
    }

    return documents.value.filter((document) =>
      [
        document.name,
        document.originalFilename,
        document.owner,
        document.id,
      ].some((value) => value.toLowerCase().includes(query)),
    );
  });

  const totalVersions = computed(() =>
    documents.value.reduce(
      (sum, document) => sum + document.versions.length,
      0,
    ),
  );

  function selectDocument(document: Document) {
    selectedDocumentId.value = document.id;
    selectedVersionId.value = document.versions[0]?.id ?? "";
  }

  function selectVersion(version: Version) {
    selectedVersionId.value = version.id;
  }

  return {
    documents,
    selectedDocumentId,
    selectedVersionId,
    selectedDocument,
    selectedVersion,
    filteredDocuments,
    totalVersions,
    searchQuery,
    selectDocument,
    selectVersion,
  };
}
