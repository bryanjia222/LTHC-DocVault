<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useVault } from "../../composables/useVault";
import { useDocuments } from "../../composables/useDocuments";
import { formatByteSize } from "../../utils/mappers";

const { t } = useI18n();
const { config, repoSize } = useVault();
const { totalVersions } = useDocuments();

const repoSizeLabel = computed(() =>
  repoSize.value == null ? "-" : formatByteSize(repoSize.value),
);
</script>

<template>
  <section class="archive-view">
    <div class="archive-grid">
      <div class="surface archive-card">
        <h3>{{ t("archive.currentBackend") }}</h3>
        <span class="status-pill" data-status="synced">{{
          t(`backend.${config.backend}`)
        }}</span>

        <dl class="archive-dl">
          <div>
            <dt>{{ t("archive.repositoryDir") }}</dt>
            <dd class="mono">{{ config.repoDir }}</dd>
          </div>
          <div>
            <dt>{{ t("archive.dataDir") }}</dt>
            <dd class="mono">{{ config.dataDir }}</dd>
          </div>
          <div>
            <dt>{{ t("archive.resticVersion") }}</dt>
            <dd>{{ config.resticVersion }}</dd>
          </div>
          <div>
            <dt>{{ t("archive.bundledBinary") }}</dt>
            <dd class="mono break">{{ config.resticPath }}</dd>
          </div>
          <div>
            <dt>{{ t("archive.resticPassword") }}</dt>
            <dd>{{ t("archive.hidden") }}</dd>
          </div>
        </dl>
      </div>

      <div class="surface archive-card">
        <h3>{{ t("archive.snapshotStats") }}</h3>
        <div class="stat-grid">
          <div class="stat">
            <span class="stat-label">{{ t("archive.snapshots") }}</span>
            <strong>{{ totalVersions }}</strong>
          </div>
          <div class="stat">
            <span class="stat-label">{{ t("archive.repoSize") }}</span>
            <strong>{{ repoSizeLabel }}</strong>
          </div>
          <div class="stat">
            <span class="stat-label">{{ t("archive.healthCheck") }}</span>
            <strong class="healthy">{{ t("archive.healthy") }}</strong>
          </div>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.archive-view {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 0;
  /* Defense-in-depth: if a future layout change lets this flex item shrink
     below its grid content, clip instead of spilling onto siblings below. */
  overflow: hidden;
}

.archive-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 18px;
}

.archive-card {
  padding: 18px;
}

.archive-card h3 {
  margin-bottom: 14px;
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.archive-dl {
  display: grid;
  gap: 12px;
  margin-top: 16px;
}

.archive-dl div {
  display: grid;
  gap: 2px;
}

.archive-dl dt {
  color: var(--text-muted);
  font-size: 12px;
}

.archive-dl dd {
  color: var(--text-primary);
  font-weight: 500;
}

.mono {
  font-family: var(--mono-font);
  font-size: 12px;
  word-break: break-all;
}

.break {
  word-break: break-all;
}

.stat-grid {
  display: grid;
  gap: 14px;
}

.stat {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--border-soft);
}

.stat:last-child {
  border-bottom: 0;
  padding-bottom: 0;
}

.stat-label {
  color: var(--text-muted);
  font-size: 13px;
}

.stat strong {
  font-size: 18px;
  color: var(--text-strong);
}

.stat strong.healthy {
  color: var(--success-text);
}
</style>
