<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useVault } from "../../composables/useVault";
import { useDocuments } from "../../composables/useDocuments";
import { formatByteSize } from "../../utils/mappers";

const { t } = useI18n();
const { config, repoSize, loadRepoSize } = useVault();
const { documents, totalVersions } = useDocuments();

const repoSizeLabel = computed(() =>
  repoSize.value == null ? "-" : formatByteSize(repoSize.value),
);

onMounted(() => {
  void loadRepoSize();
});
</script>

<template>
  <div class="surface vault-card">
    <h3>{{ t("status.vaultTitle") }}</h3>
    <div class="stat-grid">
      <div class="stat">
        <span class="stat-label">{{ t("sidebar.documents") }}</span>
        <strong>{{ documents.length }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("sidebar.versions") }}</span>
        <strong>{{ totalVersions }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("sidebar.backend") }}</span>
        <strong>{{ t(`backend.${config.backend}`) }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("archive.repoSize") }}</span>
        <strong>{{ repoSizeLabel }}</strong>
      </div>
    </div>
  </div>
</template>

<style scoped>
.vault-card {
  padding: 18px;
}

.vault-card h3 {
  margin-bottom: 14px;
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.stat-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 14px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-soft);
}

.stat:last-child {
  border-bottom: 0;
  padding-bottom: 0;
}

.stat-label {
  color: var(--text-muted);
  font-size: 12px;
}

.stat strong {
  font-size: 20px;
  color: var(--text-strong);
}
</style>
