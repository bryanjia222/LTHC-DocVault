<script setup lang="ts">
import { Loader2, Send } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import QinbixinAttachments from "./QinbixinAttachments.vue";
import RichTextEditor from "../RichTextEditor.vue";
import type { QinbixinComposeController } from "../../composables/useQinbixinCompose";

const props = defineProps<{ compose: QinbixinComposeController }>();
const { t } = useI18n();

const {
  conversations,
  sending,
  uploadingMedia,
  sendTitle,
  sendContent,
  sendFeedback,
  sendRecipientId,
  pendingMedia,
  submitMessage,
  pickMedia,
  removeMedia,
} = props.compose;
</script>

<template>
  <form class="compose-view" @submit.prevent="submitMessage">
    <div class="compose-form">
      <label class="field">
        <span>{{ t("qinbixin.recipient") }}</span>
        <select v-model="sendRecipientId" class="text-input">
          <option :value="null" disabled>
            {{ t("qinbixin.selectRecipient") }}
          </option>
          <option
            v-for="conversation in conversations"
            :key="conversation.id"
            :value="conversation.id"
          >
            {{ conversation.title }}
          </option>
        </select>
      </label>
      <input
        v-model="sendTitle"
        class="text-input"
        type="text"
        :placeholder="t('qinbixin.titlePlaceholder')"
      />
      <RichTextEditor
        v-model="sendContent"
        class="rich-editor"
        :placeholder="t('qinbixin.contentPlaceholder')"
      />
      <QinbixinAttachments
        :items="pendingMedia"
        :uploading="uploadingMedia"
        @pick="pickMedia"
        @remove="removeMedia"
      >
        <span v-if="sendFeedback" class="feedback">{{ sendFeedback }}</span>
        <button
          class="primary send-button"
          type="button"
          :disabled="sending || uploadingMedia"
          @click="submitMessage"
        >
          <Loader2 v-if="sending" class="spin" aria-hidden="true" />
          <Send v-else class="small-icon" aria-hidden="true" />
          {{ t("qinbixin.send") }}
        </button>
      </QinbixinAttachments>
    </div>
  </form>
</template>

<style scoped>
.compose-view {
  flex: 1;
  min-height: 0;
  overflow: auto;
  user-select: text;
  -webkit-user-select: text;
}

.compose-form {
  display: grid;
  gap: 8px;
}

.field {
  display: grid;
  gap: 6px;
}

.field > span {
  color: var(--text-muted);
  font-size: 12px;
}

.text-input {
  width: 100%;
  min-width: 0;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font: inherit;
}

.text-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.rich-editor {
  min-width: 0;
}

.rich-editor :deep(.tox-tinymce) {
  border-color: var(--border-strong);
  border-radius: var(--radius-sm);
}

.send-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 34px;
  padding: 0 16px;
}

.feedback {
  overflow: hidden;
  color: var(--text-muted);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.small-icon {
  width: 14px;
  height: 14px;
}
</style>
