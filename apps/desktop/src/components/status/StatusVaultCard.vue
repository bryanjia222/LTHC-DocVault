<script setup lang="ts">
import { onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { useVault } from "../../composables/useVault";
import { useDocuments } from "../../composables/useDocuments";
import { useJobs } from "../../composables/useJobs";
import { useDialogs } from "../../composables/useDialogs";

const { t } = useI18n();
const { loadRepoSize } = useVault();
const { documents, totalVersions } = useDocuments();
const { activeJobCount } = useJobs();
const { openSwitchBackend } = useDialogs();

onMounted(() => {
  void loadRepoSize();
});
</script>

<template>
  <div class="surface vault-card">
    <div class="vault-card-head">
      <h3>{{ t("status.vaultTitle") }}</h3>
      <button
        class="primary switch-button"
        type="button"
        @click="openSwitchBackend"
      >
        {{ t("connect.switchAction") }}
      </button>
    </div>
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
    </div>
  </div>
</template>

<style scoped>
.vault-card {
  padding: 18px;
}

.vault-card-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 14px;
}

.vault-card h3 {
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.switch-button {
  height: 30px;
  padding: 0 14px;
  font-size: 12px;
  white-space: nowrap;
}

.stat-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}

.stat {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-soft);
}

.stat:nth-last-child(-n + 2) {
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
