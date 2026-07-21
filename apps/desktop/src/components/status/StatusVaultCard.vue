<script setup lang="ts">
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useVault } from "../../composables/useVault";
import { useDocuments } from "../../composables/useDocuments";
import { useJobs } from "../../composables/useJobs";
import { formatByteSize } from "../../utils/mappers";

const { t } = useI18n();
const { config, repoSize, loadRepoSize } = useVault();
const { documents, totalVersions } = useDocuments();
const { activeJobCount } = useJobs();

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
        <span class="stat-label">{{ t("metrics.currentDocuments") }}</span>
        <strong>{{ documents.length }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("metrics.storedVersions") }}</span>
        <strong>{{ totalVersions }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("metrics.activeJobs") }}</span>
        <strong>{{ activeJobCount }}</strong>
      </div>
      <div class="stat">
        <span class="stat-label">{{ t("metrics.vaultHealth") }}</span>
        <strong>{{ t("metrics.ready") }}</strong>
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
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-soft);
}

.stat:nth-last-child(-n + 3) {
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
