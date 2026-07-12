<script setup lang="ts">
import { Moon, Sun } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useTheme } from "../theme";
import { useVaultActions } from "../composables/useVaultActions";
import { useCommandPalette } from "../composables/useCommandPalette";

const { t } = useI18n();
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
      <button class="secondary" type="button" @click="open">
        {{ t("actions.commandPalette") }}
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
</style>
