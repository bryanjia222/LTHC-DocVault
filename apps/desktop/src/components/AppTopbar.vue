<script setup lang="ts">
import { Moon, Sun } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { supportedLocales } from "../i18n";
import { useTheme } from "../theme";
import { useVaultActions } from "../composables/useVaultActions";
import { useCommandPalette } from "../composables/useCommandPalette";

const { t, locale } = useI18n();
const { isDark } = useTheme();
const { runAction, toggleCurrentTheme } = useVaultActions();
const { open } = useCommandPalette();
</script>

<template>
  <header class="topbar">
    <div>
      <h1>{{ t("page.title") }}</h1>
      <p>{{ t("page.subtitle") }}</p>
    </div>

    <div class="toolbar">
      <select
        v-model="locale"
        class="locale-select"
        :aria-label="t('actions.language')"
      >
        <option
          v-for="supportedLocale in supportedLocales"
          :key="supportedLocale.code"
          :value="supportedLocale.code"
        >
          {{ supportedLocale.label }}
        </option>
      </select>
      <button
        class="icon-button secondary"
        type="button"
        :title="t('actions.toggleTheme')"
        :aria-label="t('actions.toggleTheme')"
        @click="toggleCurrentTheme"
      >
        <Moon v-if="!isDark" aria-hidden="true" />
        <Sun v-else aria-hidden="true" />
      </button>
      <button
        class="secondary"
        type="button"
        @click="runAction('actionLogs.refresh')"
      >
        {{ t("actions.refresh") }}
      </button>
      <button class="secondary command-button" type="button" @click="open">
        {{ t("actions.commandPalette") }}
        <kbd>Ctrl K</kbd>
      </button>
      <button
        class="primary"
        type="button"
        @click="runAction('actionLogs.commit')"
      >
        {{ t("actions.commit") }}
      </button>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.topbar p {
  color: var(--text-muted);
}

h1 {
  font-size: 30px;
  font-weight: 750;
}

.locale-select {
  height: 34px;
  padding: 0 30px 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
}

.command-button {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.command-button kbd {
  padding: 1px 6px;
  border: 1px solid var(--border-strong);
  border-radius: 4px;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-family: var(--mono-font);
  font-size: 11px;
}
</style>
