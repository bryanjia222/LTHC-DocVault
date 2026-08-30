<script setup lang="ts">
import { X } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import BaseModal from "./BaseModal.vue";
import QinbixinCompose from "./qinbixin/QinbixinCompose.vue";
import QinbixinLoginPanel from "./qinbixin/QinbixinLoginPanel.vue";
import QinbixinMailbox from "./qinbixin/QinbixinMailbox.vue";
import { useQinbixin } from "../composables/useQinbixin";
import { useQinbixinCompose } from "../composables/useQinbixinCompose";
import { useQinbixinMailbox } from "../composables/useQinbixinMailbox";

type QinbixinView = "inbox" | "outbox" | "compose";

const props = defineProps<{
  open: boolean;
  initialView?: QinbixinView;
}>();

const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const { status, error, uploadError } = useQinbixin();

const mailbox = useQinbixinMailbox(props);
const compose = useQinbixinCompose(mailbox.activeView);
const { activeView, dialogTitle } = mailbox;

function setView(view: QinbixinView) {
  mailbox.setActiveView(view);
}

function clearUploadError() {
  uploadError.value = "";
}
</script>

<template>
  <BaseModal
    :open="props.open"
    :title="dialogTitle"
    :subtitle="
      status.logged_in ? status.profile?.nickname : t('qinbixin.loginSubtitle')
    "
    wide
    @close="emit('close')"
  >
    <QinbixinLoginPanel v-if="!status.logged_in" />

    <div v-else class="mail-root">
      <nav class="mail-tabs">
        <button
          type="button"
          :class="{ active: activeView === 'inbox' }"
          @click="setView('inbox')"
        >
          {{ t("qinbixin.inboxTab") }}
        </button>
        <button
          type="button"
          :class="{ active: activeView === 'outbox' }"
          @click="setView('outbox')"
        >
          {{ t("qinbixin.outboxTab") }}
        </button>
        <button
          type="button"
          :class="{ active: activeView === 'compose' }"
          @click="setView('compose')"
        >
          {{ t("qinbixin.composeTab") }}
        </button>
      </nav>

      <QinbixinMailbox
        v-if="activeView !== 'compose'"
        :mailbox="mailbox"
      />
      <QinbixinCompose v-else :compose="compose" />
    </div>

    <div v-if="uploadError" class="upload-error">
      <span>{{ uploadError }}</span>
      <button type="button" @click="clearUploadError">
        <X aria-hidden="true" />
      </button>
    </div>
    <p v-if="error" class="backend-error">{{ error }}</p>
  </BaseModal>
</template>

<style scoped>
.mail-root {
  display: flex;
  flex-direction: column;
  gap: 14px;
  height: min(72vh, 680px);
  min-height: 420px;
  user-select: text;
  -webkit-user-select: text;
}

.mail-tabs {
  flex-shrink: 0;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 4px;
  padding: 3px;
  overflow: hidden;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
}

.mail-tabs button {
  height: 30px;
  padding: 0 10px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--text-secondary);
  font: inherit;
  font-size: 13px;
  cursor: pointer;
}

.mail-tabs button.active {
  background: var(--bg-surface);
  color: var(--accent-text);
}

.mail-tabs button:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: -2px;
}

.backend-error {
  margin-top: 10px;
  color: var(--danger-text);
  font-size: 12px;
}

.upload-error {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  padding: 8px 12px;
  border-radius: 6px;
  background: rgba(220, 38, 38, 0.1);
  border: 1px solid rgba(220, 38, 38, 0.3);
  color: var(--danger-text);
  font-size: 12px;
}

.upload-error button {
  display: grid;
  place-items: center;
  width: 20px;
  height: 20px;
  padding: 0;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  flex-shrink: 0;
}
</style>
