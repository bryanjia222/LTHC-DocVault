<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { Mail, MoreVertical } from "@lucide/vue";
import type { QinbixinStatus } from "../composables/useQinbixin";

defineProps<{ status: QinbixinStatus }>();

const emit = defineEmits<{
  open: [];
  openMenu: [event: MouseEvent];
}>();

const { t } = useI18n();
</script>

<template>
  <div class="nav-row qinbixin-row">
    <button
      class="nav-main"
      type="button"
      :title="t('qinbixin.openInbox')"
      @click="emit('open')"
    >
      <Mail class="nav-icon" aria-hidden="true" />
      <span class="qinbixin-name">{{ t("qinbixin.title") }}</span>
      <span class="qinbixin-state">
        {{
          status.logged_in
            ? status.profile?.nickname || t("qinbixin.loggedIn")
            : t("qinbixin.loggedOut")
        }}
      </span>
      <span v-if="status.logged_in && status.has_unread" class="qinbixin-dot" />
    </button>
    <button
      class="icon-btn kebab-btn"
      type="button"
      :title="t('sidebar.moreActions')"
      :aria-label="t('sidebar.moreActions')"
      @click.stop.prevent="emit('openMenu', $event)"
    >
      <MoreVertical class="nav-icon" aria-hidden="true" />
    </button>
  </div>
</template>

<style scoped>
.nav-row {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-width: 0;
  height: 38px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: transparent;
  text-align: left;
  color: var(--text-primary);
}

.nav-row:hover {
  background: var(--bg-hover);
}

.nav-main {
  display: flex;
  flex: 1;
  align-items: center;
  gap: 8px;
  min-width: 0;
  height: 100%;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  text-align: left;
  font: inherit;
  cursor: pointer;
}

.icon-btn {
  display: grid;
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
}

.icon-btn:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.kebab-btn {
  opacity: 0;
  transition: opacity 0.12s ease;
}

.nav-row:hover .kebab-btn,
.nav-row:focus-within .kebab-btn {
  opacity: 1;
}

.qinbixin-row {
  border-bottom: 1px solid var(--border-soft);
  margin-bottom: 6px;
  padding-bottom: 8px;
}

.qinbixin-name {
  flex-shrink: 0;
}

.qinbixin-state {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  color: var(--text-muted);
  font-size: 12px;
  text-align: right;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qinbixin-dot {
  flex-shrink: 0;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--danger);
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
</style>
