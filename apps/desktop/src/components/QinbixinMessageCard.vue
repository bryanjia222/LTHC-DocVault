<script setup lang="ts">
import { computed, ref } from "vue";
import { Loader2, MessageCircle } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useQinbixin, type QinbixinMessage } from "../composables/useQinbixin";

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
const {
  addComment,
  clearCommentError,
  commentError,
  commentsByMessage,
  loadMessageComments,
  loadingComments,
  markMessageCommentsRead,
  unreadCommentCount,
} = useQinbixin();

const commentsOpen = ref(false);
const commentDraft = ref("");
const submittingComment = ref(false);

const unreadComments = computed(() => unreadCommentCount(props.message));
const comments = computed(
  () => commentsByMessage.value[props.message.id] ?? [],
);
const loadingCurrentComments = computed(
  () => loadingComments.value[props.message.id] ?? false,
);

async function toggleComments(): Promise<void> {
  commentsOpen.value = !commentsOpen.value;
  if (!commentsOpen.value) return;
  clearCommentError();
  await loadMessageComments(props.message.id);
  if (!commentError.value) {
    markMessageCommentsRead(props.message);
  }
}

async function submitComment(): Promise<void> {
  if (!commentDraft.value.trim() || submittingComment.value) return;
  submittingComment.value = true;
  const result = await addComment(props.message.id, commentDraft.value);
  submittingComment.value = false;
  if (result.success) {
    commentDraft.value = "";
    markMessageCommentsRead(props.message);
  }
}

function onContentClickCapture(event: MouseEvent): void {
  emit("open-link", event);
}
</script>

<template>
  <article class="message" :class="{ incoming: props.incoming }">
    <header class="message-header">
      <span class="message-title-wrap">
        <strong>{{ props.message.title }}</strong>
        <span v-if="unreadComments > 0" class="message-dot" />
      </span>
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
    <!-- eslint-disable vue/no-v-html --
      safeContent is DOMPurify-sanitized in QinbixinDialog before it reaches
      this card. The disable spans the multi-line element because
      disable-next-line only covers the immediately following line. -->
    <div
      class="message-content"
      @click.capture="onContentClickCapture"
      v-html="props.message.safeContent"
    />
    <!-- eslint-enable vue/no-v-html -->
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

    <div class="reply-row">
      <button
        class="reply-button"
        type="button"
        :title="t('qinbixin.reply')"
        @click="toggleComments"
      >
        <MessageCircle aria-hidden="true" />
        <span>{{ t("qinbixin.reply") }}</span>
        <span v-if="props.message.comment_count > 0" class="reply-count">
          {{ props.message.comment_count }}
        </span>
        <span v-if="unreadComments > 0" class="reply-dot" />
      </button>
    </div>

    <div v-if="commentsOpen" class="comments-panel">
      <div v-if="loadingCurrentComments" class="comments-state">
        <Loader2 class="spin" aria-hidden="true" />
      </div>
      <template v-else>
        <p v-if="comments.length === 0" class="comments-state">
          {{ t("qinbixin.noComments") }}
        </p>
        <ol v-else class="comment-list">
          <li v-for="comment in comments" :key="comment.id">
            <img
              v-if="comment.avatar"
              class="comment-avatar"
              :src="comment.avatar"
              :alt="comment.author"
            />
            <div v-else class="comment-avatar comment-avatar-fallback">
              {{ comment.author.slice(0, 1) }}
            </div>
            <div class="comment-body">
              <div class="comment-meta">
                <span>{{ comment.author }}</span>
                <span>{{ comment.sent_time }}</span>
              </div>
              <p>{{ comment.content }}</p>
              <div v-if="comment.images.length" class="comment-images">
                <button
                  v-for="image in comment.images"
                  :key="image"
                  type="button"
                  @click="emit('preview-image', image)"
                >
                  <img :src="image" :alt="comment.author" />
                </button>
              </div>
            </div>
          </li>
        </ol>
      </template>
      <p v-if="commentError" class="comment-error">{{ commentError }}</p>
      <form class="comment-form" @submit.prevent="submitComment">
        <input
          v-model="commentDraft"
          type="text"
          :placeholder="t('qinbixin.commentPlaceholder')"
        />
        <button
          type="submit"
          :disabled="submittingComment || !commentDraft.trim()"
        >
          {{ t("qinbixin.commentSend") }}
        </button>
      </form>
    </div>
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

.message-title-wrap {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}

.message-dot,
.reply-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--danger);
  flex-shrink: 0;
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

.reply-row {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
}

.reply-button {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 9px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-subtle);
  color: var(--text-secondary);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.reply-button:hover {
  background: var(--bg-hover);
  color: var(--text-primary);
}

.reply-count {
  color: var(--text-muted);
}

.reply-dot {
  position: absolute;
  top: -3px;
  right: -3px;
}

.comments-panel {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border-soft);
}

.comments-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 32px;
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.spin {
  width: 14px;
  height: 14px;
  animation: qinbixin-comment-spin 0.8s linear infinite;
}

.comment-list {
  display: grid;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
}

.comment-list li {
  display: flex;
  gap: 8px;
}

.comment-avatar,
.comment-avatar-fallback {
  display: grid;
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  place-items: center;
  border-radius: 50%;
  object-fit: cover;
  background: var(--bg-inset);
  color: var(--text-secondary);
  font-size: 11px;
}

.comment-body {
  min-width: 0;
}

.comment-meta {
  display: flex;
  gap: 8px;
  color: var(--text-muted);
  font-size: 11px;
}

.comment-body p {
  margin: 3px 0 0;
  color: var(--text-secondary);
  overflow-wrap: anywhere;
  white-space: pre-wrap;
}

.comment-images {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 6px;
}

.comment-images button {
  padding: 0;
  border: 0;
  background: transparent;
  cursor: zoom-in;
}

.comment-images img {
  display: block;
  width: 56px;
  height: 56px;
  object-fit: cover;
  border-radius: 4px;
}

.comment-form {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.comment-error {
  margin: 8px 0 0;
  color: var(--danger-text);
  font-size: 12px;
}

.comment-form input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 9px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-surface);
  color: var(--text-primary);
  font: inherit;
  font-size: 12px;
}

.comment-form button {
  height: 30px;
  padding: 0 10px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-surface);
  color: var(--accent-text);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}

.comment-form button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

@keyframes qinbixin-comment-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
