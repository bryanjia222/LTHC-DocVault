<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { supportedLocales } from "../../i18n";
import { useTheme } from "../../theme";
import { useVault } from "../../composables/useVault";
import { useNavigation, type SettingsTab } from "../../composables/useNavigation";
import StageResetSlider from "../StageResetSlider.vue";
import { useDevMode } from "../../composables/useDevMode";
import { useDoubleClickPref } from "../../composables/useDoubleClickPref";
import {
  useTableColumns,
  ALL_COLUMN_IDS,
  type ColumnId,
} from "../../composables/useTableColumns";
import StatusVaultCard from "../status/StatusVaultCard.vue";
import StatusTasksPanel from "../status/StatusTasksPanel.vue";
import StatusArchivePanel from "../status/StatusArchivePanel.vue";

const { t, locale } = useI18n();
const { isDark, setTheme } = useTheme();
const { config, setLogLevel } = useVault();
const { settingsTab } = useNavigation();
const { isDevMode } = useDevMode();
const { doubleClickAction, setDoubleClickAction } = useDoubleClickPref();
const { columns, setVisible, resetColumns, isAlwaysVisible } =
  useTableColumns();

function onToggleColumn(id: ColumnId, event: Event) {
  setVisible(id, (event.target as HTMLInputElement).checked);
}

// Dev-only reset card: vite strips this in production builds, so the
// destructive test actions never ship to end users.
const isDev = import.meta.env.DEV;

const logLevels = ["error", "warn", "info", "debug", "trace"] as const;

async function onLogLevelChange(event: Event) {
  const level = (event.target as HTMLSelectElement).value;
  try {
    await setLogLevel(level);
  } catch (e) {
    console.error("set_log_level failed", e);
  }
}

const tabs: { id: SettingsTab; labelKey: string }[] = [
  { id: "status", labelKey: "settings.tabs.status" },
  { id: "appearance", labelKey: "settings.tabs.appearance" },
];
</script>

<template>
  <section class="settings-view">
    <div class="settings-intro surface">
      <h2>{{ t("settings.title") }}</h2>
      <p class="subtitle">{{ t("settings.subtitle") }}</p>
    </div>

    <div class="tab-bar" role="tablist">
      <button
        v-for="tab in tabs"
        :key="tab.id"
        type="button"
        role="tab"
        :aria-selected="settingsTab === tab.id"
        :class="{ active: settingsTab === tab.id }"
        @click="settingsTab = tab.id"
      >
        {{ t(tab.labelKey) }}
      </button>
    </div>

    <!-- 状态: vault summary (with switch-backend) + tasks + archive + database/
         logging. The old "存储" tab was folded in here and de-duplicated: the
         backend / repo size / storage paths already lived on the archive panel,
         so only the database path and logging controls that were unique to it
         move into this tab. v-show keeps every pane mounted so switching tabs
         never pays a teardown/remount cost. -->
    <div v-show="settingsTab === 'status'" class="tab-pane status-pane">
      <StatusVaultCard />
      <StatusTasksPanel />
      <StatusArchivePanel />

      <div class="surface settings-card status-detail-card">
        <h3>{{ t("settings.databaseSection") }}</h3>
        <dl class="settings-dl">
          <div>
            <dt>{{ t("settings.dbPath") }}</dt>
            <dd class="mono break">{{ config.dbPath }}</dd>
          </div>
        </dl>

        <h3 class="logging-title">{{ t("settings.loggingSection") }}</h3>
        <dl class="settings-dl">
          <div>
            <dt>{{ t("settings.logLevel") }}</dt>
            <dd>
              <select
                class="locale-select"
                :value="config.logLevel"
                @change="onLogLevelChange"
              >
                <option v-for="level in logLevels" :key="level" :value="level">
                  {{ level }}
                </option>
              </select>
            </dd>
          </div>
          <div>
            <dt>{{ t("settings.logFile") }}</dt>
            <dd class="mono break">{{ config.logFile }}</dd>
          </div>
        </dl>
      </div>
    </div>

    <!-- 外观: theme / language / dev mode (+ dev reset card). -->
    <div v-show="settingsTab === 'appearance'" class="tab-pane">
      <div class="settings-grid single">
        <div class="surface settings-card">
          <h3>{{ t("settings.appearanceSection") }}</h3>
          <dl class="settings-dl">
            <div>
              <dt>{{ t("settings.theme") }}</dt>
              <dd>
                <div class="segmented-control">
                  <button
                    type="button"
                    :class="{ active: !isDark }"
                    @click="setTheme('light')"
                  >
                    {{ t("settings.themeLight") }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: isDark }"
                    @click="setTheme('dark')"
                  >
                    {{ t("settings.themeDark") }}
                  </button>
                </div>
              </dd>
            </div>
            <div>
              <dt>{{ t("settings.language") }}</dt>
              <dd>
                <select v-model="locale" class="locale-select">
                  <option
                    v-for="supportedLocale in supportedLocales"
                    :key="supportedLocale.code"
                    :value="supportedLocale.code"
                  >
                    {{ supportedLocale.label }}
                  </option>
                </select>
              </dd>
            </div>
            <div>
              <dt>{{ t("settings.devMode") }}</dt>
              <dd>
                <div class="segmented-control">
                  <button
                    type="button"
                    :class="{ active: !isDevMode }"
                    @click="isDevMode = false"
                  >
                    {{ t("settings.off") }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: isDevMode }"
                    @click="isDevMode = true"
                  >
                    {{ t("settings.on") }}
                  </button>
                </div>
                <p class="field-hint">{{ t("settings.devModeHint") }}</p>
              </dd>
            </div>
            <div>
              <dt>{{ t("settings.doubleClick") }}</dt>
              <dd>
                <div class="segmented-control">
                  <button
                    type="button"
                    :class="{ active: doubleClickAction === 'preview' }"
                    @click="setDoubleClickAction('preview')"
                  >
                    {{ t("settings.doubleClickPreview") }}
                  </button>
                  <button
                    type="button"
                    :class="{ active: doubleClickAction === 'open' }"
                    @click="setDoubleClickAction('open')"
                  >
                    {{ t("settings.doubleClickOpen") }}
                  </button>
                </div>
                <p class="field-hint">{{ t("settings.doubleClickHint") }}</p>
              </dd>
            </div>
          </dl>
        </div>
      </div>

      <div class="surface settings-card">
        <h3>{{ t("settings.columnsSection") }}</h3>
        <p class="field-hint">{{ t("settings.columnsHint") }}</p>
        <div class="column-toggles">
          <label
            v-for="id in ALL_COLUMN_IDS"
            :key="id"
            class="column-toggle"
            :class="{ disabled: isAlwaysVisible(id) }"
          >
            <input
              type="checkbox"
              :checked="columns[id].visible"
              :disabled="isAlwaysVisible(id)"
              @change="onToggleColumn(id, $event)"
            />
            <span>{{ t(`documents.columns.${id}`) }}</span>
            <span v-if="isAlwaysVisible(id)" class="always-on-tag">{{
              t("settings.columnsAlwaysOn")
            }}</span>
          </label>
        </div>
        <button type="button" class="columns-reset" @click="resetColumns">
          {{ t("settings.columnsReset") }}
        </button>
      </div>

      <div v-if="isDev" class="surface settings-card dev-card">
        <h3>{{ t("dev.title") }}</h3>
        <p class="field-hint">{{ t("dev.hint") }}</p>
        <StageResetSlider />
      </div>
    </div>
  </section>
</template>

<style scoped>
.settings-view {
  display: flex;
  flex-direction: column;
  gap: 18px;
  min-height: 0;
  overflow: auto;
}

.settings-intro {
  padding: 20px;
}

.settings-intro h2 {
  font-size: 18px;
}

.settings-intro .subtitle {
  margin-top: 4px;
  color: var(--text-muted);
}

.tab-bar {
  display: flex;
  gap: 4px;
  padding: 4px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius);
  background: var(--bg-inset);
}

.tab-bar button {
  flex: 1;
  height: 34px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-weight: 600;
  cursor: pointer;
}

.tab-bar button:hover {
  background: var(--bg-hover);
}

.tab-bar button.active {
  background: var(--bg-surface);
  color: var(--text-primary);
  box-shadow: var(--overlay-shadow);
}

.tab-pane {
  display: flex;
  flex-direction: column;
  gap: 18px;
  /* Keep each pane at its natural height so .settings-view (overflow:auto)
     scrolls instead of letting a flex child shrink below its content - which
     previously caused the archive cards to spill out and overlap the
     database/logging card below. */
  flex-shrink: 0;
}

.settings-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 18px;
}

.settings-grid.single {
  grid-template-columns: minmax(0, 1fr);
}

.settings-card {
  padding: 18px;
}

.settings-card h3 {
  margin-bottom: 14px;
  font-size: 13px;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.logging-title {
  margin-top: 20px;
}

.settings-dl {
  display: grid;
  gap: 12px;
}

.settings-dl div {
  display: grid;
  gap: 2px;
}

.settings-dl dt {
  color: var(--text-muted);
  font-size: 12px;
}

.settings-dl dd {
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

.locale-select {
  height: 32px;
  padding: 0 28px 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
}

.segmented-control {
  grid-template-columns: repeat(2, auto);
}

.segmented-control button {
  height: 32px;
  min-width: 64px;
  padding: 0 14px;
}

.field-hint {
  margin: 6px 0 0;
  color: var(--text-muted);
  font-size: 12px;
}

.dev-card {
  padding: 18px;
}

.column-toggles {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 18px;
  margin-top: 6px;
}

.column-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary);
  cursor: pointer;
}

.column-toggle.disabled {
  color: var(--text-muted);
  cursor: default;
}

.column-toggle input {
  margin: 0;
}

.always-on-tag {
  margin-left: auto;
  padding: 1px 6px;
  border-radius: 4px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 11px;
}

.columns-reset {
  margin-top: 14px;
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
}

.columns-reset:hover {
  background: var(--bg-hover);
}
</style>
