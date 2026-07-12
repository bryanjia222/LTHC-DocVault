<script setup lang="ts">
import { Archive, FileText, Settings, Activity } from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { navigationItems, useNavigation } from "../composables/useNavigation";
import { useVaultActions } from "../composables/useVaultActions";
import { useDocuments } from "../composables/useDocuments";
import { useVault } from "../composables/useVault";

const { t } = useI18n();
const { activeSection } = useNavigation();
const { navigate } = useVaultActions();
const { documents, totalVersions } = useDocuments();
const { config } = useVault();

const navIcons = {
  documents: FileText,
  jobs: Activity,
  archive: Archive,
  settings: Settings,
} as const;
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark">DV</div>
      <div>
        <strong>DocVault</strong>
        <span>{{ t("app.tagline") }}</span>
      </div>
    </div>

    <nav class="nav-list" :aria-label="t('nav.primary')">
      <button
        v-for="item in navigationItems"
        :key="item.id"
        :class="{ active: activeSection === item.id }"
        type="button"
        :aria-current="activeSection === item.id ? 'page' : undefined"
        @click="navigate(item.id)"
      >
        <component
          :is="navIcons[item.id]"
          class="nav-icon"
          aria-hidden="true"
        />
        {{ t(item.labelKey) }}
      </button>
    </nav>

    <div class="sidebar-block">
      <div class="block-title">{{ t("sidebar.vault") }}</div>
      <dl>
        <div>
          <dt>{{ t("sidebar.documents") }}</dt>
          <dd>{{ documents.length }}</dd>
        </div>
        <div>
          <dt>{{ t("sidebar.versions") }}</dt>
          <dd>{{ totalVersions }}</dd>
        </div>
        <div>
          <dt>{{ t("sidebar.backend") }}</dt>
          <dd>{{ t(`backend.${config.backend}`) }}</dd>
        </div>
      </dl>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  display: flex;
  flex-direction: column;
  gap: 24px;
  padding: 20px;
  border-right: 1px solid var(--border);
  background: var(--bg-sidebar);
}

.brand {
  display: flex;
  align-items: center;
  gap: 12px;
}

.brand-mark {
  display: grid;
  width: 40px;
  height: 40px;
  place-items: center;
  border-radius: var(--radius);
  background: var(--brand);
  color: #ffffff;
  font-weight: 700;
}

.brand strong,
.brand span {
  display: block;
}

.brand span {
  color: var(--text-muted);
}

.nav-list {
  display: grid;
  gap: 6px;
}

.nav-list button {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  height: 38px;
  padding: 0 12px;
  border-color: transparent;
  background: transparent;
  text-align: left;
  color: var(--text-primary);
}

.nav-list button:hover {
  background: var(--bg-hover);
  border-color: transparent;
}

.nav-list button.active {
  background: var(--bg-active);
  color: var(--accent-text);
  font-weight: 650;
}

.nav-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  fill: none;
  stroke: currentcolor;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 2;
}

.sidebar-block {
  margin-top: auto;
  padding-top: 16px;
  border-top: 1px solid var(--border-soft);
}

.block-title {
  margin-bottom: 12px;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 700;
  text-transform: uppercase;
}

.sidebar-block dl {
  display: grid;
  gap: 10px;
}

.sidebar-block dl div {
  display: flex;
  justify-content: space-between;
  gap: 16px;
}
</style>
