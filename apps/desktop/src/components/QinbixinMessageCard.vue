<script setup lang="ts">
import { useI18n } from "vue-i18n";

import type { QinbixinMessage } from "../composables/useQinbixin";

const props = defineProps<{
  message: QinbixinMessage & { safeContent: string };
  incoming: boolean;
  participant: string;
}>();

const emit = defineEmits<{
  "preview-image": [url: string];
  "open-link": [event: MouseEvent];
  "open-external": [url: string];
}>();

const { t } = useI18n();

function onContentClickCapture(event: MouseEvent): void {
  emit("open-link", event);
}
</script>

<template>
  <article class="message" :class="{ incoming: props.incoming }">
    <header class="message-header">
      <strong>{{ props.message.title }}</strong>
      <span class="message-time">{{ props.message.sent_time }}</span>
    </header>
    <p class="message-participant">
      {{
        props.incoming
          ? t("qinbixin.fromParticipant")
          : t("qinbixin.toParticipant")
      }}
      {{ props.participant }}
    </p>
    <p v-if="props.message.song_title" class="message-song">
      <span>{{ t("qinbixin.songTitle") }}:</span>
      {{ props.message.song_title }}
    </p>
    <!-- eslint-disable-next-line vue/no-v-html -->
    <div
      class="message-content"
      @click.capture="onContentClickCapture"
      v-html="props.message.safeContent"
    />
    <div v-if="props.message.tags.length" class="message-tags">
      <span v-for="tag in props.message.tags" :key="tag"># {{ tag }}</span>
    </div>
    <div v-if="props.message.images.length" class="media-grid">
      <button
        v-for="image in props.message.images"
        :key="image"
        class="media-thumb"
        type="button"
        :title="t('qinbixin.previewImage')"
        @click="emit('preview-image', image)"
      >
        <img :src="image" :alt="props.message.title" />
      </button>
    </div>
    <div v-if="props.message.videos.length" class="media-grid">
      <video
        v-for="video in props.message.videos"
        :key="video"
        controls
        preload="metadata"
        :src="video"
      />
    </div>
    <a
      v-if="props.message.file_url"
      class="attachment-link"
      :href="props.message.file_url"
      @click.prevent="emit('open-external', props.message.file_url)"
    >
      {{ t("qinbixin.attachmentDownload") }}
    </a>
  </article>
</template>

<style scoped>
.message {
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  padding: 10px 12px;
  background: var(--bg-surface);
}

.message.incoming {
  background: var(--bg-subtle);
}

.message-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 6px;
}

.message-header strong {
  min-width: 0;
  overflow-wrap: anywhere;
}

.message-participant {
  margin-bottom: 6px;
  color: var(--text-muted);
  font-size: 12px;
}

.message-time {
  flex-shrink: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.message-song {
  margin-bottom: 6px;
  color: var(--text-secondary);
  font-size: 12px;
}

.message-song span {
  color: var(--text-muted);
}

.message-content {
  color: var(--text-secondary);
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.message-content :deep(p) {
  margin: 0 0 6px;
}

.message-content :deep(img) {
  display: block;
  max-width: 100%;
  height: auto;
  border-radius: var(--radius-sm);
}

.message-content :deep(video),
.message-content :deep(audio) {
  display: block;
  max-width: 100%;
  margin: 6px 0;
}

.message-content :deep(video) {
  aspect-ratio: 16 / 9;
  width: 100%;
  height: auto;
  border-radius: var(--radius-sm);
  background: var(--bg-inset);
}

.message-content :deep(blockquote) {
  margin: 6px 0;
  padding: 4px 10px;
  border-left: 3px solid var(--border-strong);
  color: var(--text-secondary);
}

.message-content :deep(pre) {
  overflow: auto;
  margin: 6px 0;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-inset);
  font-size: 12px;
}

.message-content :deep(a) {
  color: var(--accent);
}

.message-content :deep(table) {
  max-width: 100%;
  margin: 6px 0;
  border-collapse: collapse;
}

.message-content :deep(th),
.message-content :deep(td) {
  min-width: 36px;
  padding: 5px 7px;
  border: 1px solid var(--border-strong);
  vertical-align: top;
}

.message-content :deep(figure) {
  margin: 8px 0;
}

.message-content :deep(figcaption) {
  margin-top: 4px;
  color: var(--text-muted);
  font-size: 12px;
  text-align: center;
}

.message-content :deep(p:last-child) {
  margin-bottom: 0;
}

.message-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.message-tags span {
  color: var(--text-muted);
  font-size: 12px;
}

.media-grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
  margin-top: 8px;
}

.media-thumb {
  padding: 0;
  border: 0;
  overflow: hidden;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: zoom-in;
}

.media-thumb img {
  display: block;
  aspect-ratio: 1;
  width: 100%;
  height: auto;
  object-fit: cover;
}

.media-grid video {
  display: block;
  aspect-ratio: 16 / 9;
  width: 100%;
  height: auto;
  border-radius: var(--radius-sm);
  background: var(--bg-inset);
}

.attachment-link {
  display: inline-flex;
  align-items: center;
  margin-top: 8px;
  color: var(--accent);
  font-size: 12px;
  text-decoration: none;
}

.attachment-link:hover {
  text-decoration: underline;
}
</style>
