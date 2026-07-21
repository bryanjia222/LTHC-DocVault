<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { supportedLocales } from "../../i18n";
import { useTheme } from "../../theme";
import { useVault } from "../../composables/useVault";
import { useNavigation, type SettingsTab } from "../../composables/useNavigation";
import StageResetSlider from "../StageResetSlider.vue";
import { useDevMode } from "../../composables/useDevMode";
import { useDialogs } from "../../composables/useDialogs";
import StatusVaultCard from "../status/StatusVaultCard.vue";
import StatusTasksPanel from "../status/StatusTasksPanel.vue";
import StatusArchivePanel from "../status/StatusArchivePanel.vue";

const { t, locale } = useI18n();
const { isDark, setTheme } = useTheme();
const { config } = useVault();
const { settingsTab } = useNavigation();
const { isDevMode } = useDevMode();
const { openSwitchBackend } = useDialogs();

// Dev-only reset card: vite strips this in production builds, so the
// destructive test actions never ship to end users.
const isDev = import.meta.env.DEV;

const tabs: { id: SettingsTab; labelKey: string }[] = [
  { id: "status", labelKey: "settings.tabs.status" },
  { id: "storage", labelKey: "settings.tabs.storage" },
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

    <!-- 状态: vault summary + tasks + archive (the old sidebar vault block +
         the Jobs/Archive views, unified into one tab). -->
    <div v-if="settingsTab === 'status'" class="tab-pane status-pane">
      <StatusVaultCard />
      <StatusTasksPanel />
      <StatusArchivePanel />
    </div>

    <!-- 存储: switch backend + storage / database / logging paths. -->
    <div v-else-if="settingsTab === 'storage'" class="tab-pane">
      <div class="surface settings-card switch-card">
        <h3>{{ t("settings.connectSection") }}</h3>
        <p class="switch-hint">{{ t("connect.switchHint") }}</p>
        <button
          class="primary switch-button"
          type="button"
          @click="openSwitchBackend"
        >
          {{ t("connect.switchAction") }}
        </button>
      </div>

      <div class="settings-grid">
        <div class="surface settings-card">
          <h3>{{ t("settings.storageSection") }}</h3>
          <dl class="settings-dl">
            <div>
              <dt>{{ t("settings.backend") }}</dt>
              <dd>{{ t(`backend.${config.backend}`) }}</dd>
            </div>
            <div>
              <dt>{{ t("settings.dataDir") }}</dt>
              <dd class="mono">{{ config.dataDir }}</dd>
            </div>
            <div>
              <dt>{{ t("settings.repoDir") }}</dt>
              <dd class="mono">{{ config.repoDir }}</dd>
            </div>
            <div>
              <dt>{{ t("settings.resticPath") }}</dt>
              <dd class="mono break">{{ config.resticPath }}</dd>
            </div>
            <div>
              <dt>{{ t("settings.resticPassword") }}</dt>
              <dd>{{ t("settings.resticPasswordHidden") }}</dd>
            </div>
          </dl>
        </div>

        <div class="surface settings-card">
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
              <dd>{{ config.logLevel }}</dd>
            </div>
            <div>
              <dt>{{ t("settings.logFile") }}</dt>
              <dd class="mono break">{{ config.logFile }}</dd>
            </div>
          </dl>
        </div>
      </div>
    </div>

    <!-- 外观: theme / language / dev mode (+ dev reset card). -->
    <div v-else-if="settingsTab === 'appearance'" class="tab-pane">
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
          </dl>
        </div>
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
}

.status-pane {
  /* The tasks panel manages its own min-height; let the tab scroll. */
  min-height: 0;
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

.switch-card {
  padding: 18px;
}

.switch-hint {
  margin: 4px 0 14px;
  color: var(--text-muted);
  font-size: 13px;
}

.switch-button {
  height: 34px;
  padding: 0 18px;
}

.field-hint {
  margin: 6px 0 0;
  color: var(--text-muted);
  font-size: 12px;
}

.dev-card {
  padding: 18px;
}
</style>
