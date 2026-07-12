import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  documents as mockDocuments,
  type Document,
  type Version,
} from "../data/mock";

/*
 * Document selection + search state. Shared app-wide so the topbar actions and
 * command palette can reference the currently selected document/version.
 */

const documents = ref<Document[]>(
  mockDocuments.map((document) => ({ ...document })),
);
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
  const { t } = useI18n();

  const filteredDocuments = computed<Document[]>(() => {
    const query = searchQuery.value.trim().toLowerCase();

    if (!query) {
      return documents.value;
    }

    return documents.value.filter((document) =>
      [
        t(document.nameKey),
        document.originalFilename,
        t(document.ownerKey),
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
