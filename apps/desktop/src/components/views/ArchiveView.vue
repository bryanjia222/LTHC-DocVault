<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useVault } from "../../composables/useVault";
import { useDocuments } from "../../composables/useDocuments";

const { t } = useI18n();
const { config } = useVault();
const { totalVersions } = useDocuments();
</script>

<template>
  <section class="archive-view">
    <div class="archive-intro surface">
      <h2>{{ t("archive.title") }}</h2>
      <p class="subtitle">{{ t("archive.subtitle") }}</p>
      <p class="description">{{ t("archive.description") }}</p>
    </div>

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
            <strong>42 MB</strong>
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
  overflow: auto;
}

.archive-intro {
  padding: 20px;
}

.archive-intro h2 {
  font-size: 18px;
}

.archive-intro .subtitle {
  margin-top: 4px;
  color: var(--text-muted);
}

.archive-intro .description {
  margin-top: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
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
