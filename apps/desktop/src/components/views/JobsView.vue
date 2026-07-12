<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { useJobs } from "../../composables/useJobs";
import { useActivityLog } from "../../composables/useActivityLog";
import { useVaultActions } from "../../composables/useVaultActions";

const { t } = useI18n();
const { jobs } = useJobs();
const { logEntries, clear } = useActivityLog();
const { runAction } = useVaultActions();
</script>

<template>
  <section class="jobs-grid">
    <div class="jobs-panel surface">
      <div class="panel-header compact">
        <div>
          <h2>{{ t("jobs.title") }}</h2>
          <p>{{ t("jobs.subtitle") }}</p>
        </div>
        <button
          class="secondary"
          type="button"
          @click="runAction('actionLogs.jobCenter')"
        >
          {{ t("actions.viewAll") }}
        </button>
      </div>

      <div class="job-list">
        <div v-for="job in jobs" :key="job.id" class="job-row">
          <span class="job-kind">{{ t(`jobs.${job.kind}`) }}</span>
          <strong class="job-target">{{ t(job.targetKey) }}</strong>
          <div class="progress-track" aria-hidden="true">
            <span :style="{ width: `${job.progress}%` }" />
          </div>
          <em class="job-status" :data-status="job.status">{{
            t(`status.${job.status}`)
          }}</em>
        </div>
        <p v-if="jobs.length === 0" class="empty-state">
          {{ t("jobs.empty") }}
        </p>
      </div>
    </div>

    <div class="log-panel surface">
      <div class="panel-header compact">
        <div>
          <h2>{{ t("log.title") }}</h2>
          <p>{{ t("log.subtitle") }}</p>
        </div>
        <button class="secondary" type="button" @click="clear">
          {{ t("actions.clear") }}
        </button>
      </div>
      <ol>
        <li v-for="entry in logEntries" :key="entry">{{ entry }}</li>
      </ol>
    </div>
  </section>
</template>

<style scoped>
.jobs-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 420px;
  grid-template-rows: minmax(0, 1fr);
  gap: 18px;
  min-height: 0;
}

.jobs-panel,
.log-panel {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
  padding: 16px;
}

.jobs-panel h2,
.log-panel h2 {
  font-size: 18px;
}

.job-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: grid;
  gap: 8px;
  align-content: start;
}

.job-row {
  display: grid;
  grid-template-columns: 82px minmax(180px, 1fr) 180px 72px;
  align-items: center;
  gap: 12px;
  min-height: 38px;
  padding: 0 2px;
}

.job-kind {
  color: var(--text-muted);
  font-size: 12px;
}

.job-target {
  color: var(--text-primary);
}

.job-status {
  color: var(--text-muted);
  font-size: 12px;
  font-style: normal;
}

.job-status[data-status="running"] {
  color: var(--accent);
  font-weight: 650;
}

.job-status[data-status="done"] {
  color: var(--success-text);
}

.empty-state {
  margin: 0;
  padding: 28px 12px;
  color: var(--text-muted);
  font-style: italic;
  text-align: center;
}

.log-panel ol {
  flex: 1;
  min-height: 0;
  margin: 0;
  padding: 0;
  overflow: auto;
  list-style: none;
  display: grid;
  gap: 8px;
  align-content: start;
}

.log-panel li {
  min-height: 26px;
  padding: 4px 0;
  border-bottom: 1px solid var(--border-soft);
  color: var(--text-secondary);
  font-family: var(--mono-font);
  font-size: 12px;
  word-break: break-all;
}
</style>
