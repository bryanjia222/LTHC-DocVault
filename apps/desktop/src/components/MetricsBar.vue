<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useDocuments } from "../composables/useDocuments";
import { useJobs } from "../composables/useJobs";

const { t } = useI18n();
const { documents, totalVersions } = useDocuments();
const { activeJobCount } = useJobs();
</script>

<template>
  <section class="metrics" :aria-label="t('metrics.label')">
    <div class="surface metric">
      <span class="metric-label">{{ t("metrics.currentDocuments") }}</span>
      <strong>{{ documents.length }}</strong>
    </div>
    <div class="surface metric">
      <span class="metric-label">{{ t("metrics.storedVersions") }}</span>
      <strong>{{ totalVersions }}</strong>
    </div>
    <div class="surface metric">
      <span class="metric-label">{{ t("metrics.activeJobs") }}</span>
      <strong>{{ activeJobCount }}</strong>
    </div>
    <div class="surface metric">
      <span class="metric-label">{{ t("metrics.vaultHealth") }}</span>
      <strong>{{ t("metrics.ready") }}</strong>
    </div>
  </section>
</template>

<style scoped>
.metrics {
  display: grid;
  grid-template-columns: repeat(4, minmax(150px, 1fr));
  gap: 16px;
}

.metric {
  display: grid;
  gap: 4px;
  padding: 14px 16px;
}

.metric strong {
  font-size: 22px;
  color: var(--text-strong);
}
</style>
