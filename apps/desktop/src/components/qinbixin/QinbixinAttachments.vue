<script setup lang="ts">
import { File, Image, Paperclip, Video, X } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import type { QinbixinPendingMedia } from "../../composables/useQinbixinCompose";

defineProps<{
  items: QinbixinPendingMedia[];
  uploading: boolean;
}>();

const emit = defineEmits<{
  pick: [kind: QinbixinPendingMedia["kind"]];
  remove: [localPath: string];
}>();

const { t } = useI18n();
</script>

<template>
  <div v-if="items.length" class="pending-media">
    <div v-for="item in items" :key="item.localPath" class="media-chip">
      <div class="media-thumb">
        <img
          v-if="item.kind === 'image' && item.thumb"
          :src="item.thumb"
          :alt="item.title"
        />
        <video
          v-else-if="item.kind === 'video' && item.thumb"
          :src="item.thumb"
          preload="metadata"
        />
        <File v-else aria-hidden="true" />
        <div
          v-if="item.progress < 100"
          class="progress-ring"
          :style="{
            background: `conic-gradient(var(--accent) ${item.progress * 3.6}deg, rgba(0, 0, 0, 0.45) ${item.progress * 3.6}deg)`,
          }"
        >
          <span>{{ Math.round(item.progress) }}%</span>
        </div>
      </div>
      <span class="media-title">{{ item.title }}</span>
      <button
        type="button"
        :title="t('qinbixin.removeAttachment')"
        @click="emit('remove', item.localPath)"
      >
        <X aria-hidden="true" />
      </button>
    </div>
  </div>

  <div class="compose-actions">
    <div class="media-actions">
      <button
        class="icon-button media-button"
        type="button"
        :title="t('qinbixin.addImage')"
        :disabled="uploading"
        @click="emit('pick', 'image')"
      >
        <Image aria-hidden="true" />
      </button>
      <button
        class="icon-button media-button"
        type="button"
        :title="t('qinbixin.addVideo')"
        :disabled="uploading"
        @click="emit('pick', 'video')"
      >
        <Video aria-hidden="true" />
      </button>
      <button
        class="icon-button media-button"
        type="button"
        :title="t('qinbixin.addAttachment')"
        :disabled="uploading"
        @click="emit('pick', 'file')"
      >
        <Paperclip aria-hidden="true" />
      </button>
    </div>
    <slot />
  </div>
</template>

<style scoped>
.media-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-right: auto;
}

.compose-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.media-button {
  width: 30px;
  height: 30px;
}

.pending-media {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.media-chip {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  position: relative;
  width: 72px;
}

.media-thumb {
  position: relative;
  width: 64px;
  height: 64px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border-soft);
  background: var(--bg-subtle);
}

.media-thumb img,
.media-thumb video {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.progress-ring {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: rgba(0, 0, 0, 0.35);
  color: #fff;
  font-size: 12px;
  font-weight: 600;
}

.media-title {
  max-width: 72px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
  color: var(--text-secondary);
}

.media-chip button {
  position: absolute;
  top: -4px;
  right: -4px;
  display: grid;
  width: 18px;
  height: 18px;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 50%;
  background: var(--bg-subtle);
  color: var(--text-muted);
  cursor: pointer;
  z-index: 1;
}

.media-chip button svg {
  width: 12px;
  height: 12px;
}
</style>
