<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import DOMPurify from "dompurify";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { File, Image, Loader2, Paperclip, Send, Video, X } from "@lucide/vue";

import BaseModal from "./BaseModal.vue";
import { openUrl } from "../composables/useVault";
import { useQinbixin, type QinbixinMedia } from "../composables/useQinbixin";

const props = defineProps<{
  open: boolean;
}>();

const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const {
  status,
  conversations,
  selectedConversationId,
  selectedConversation,
  messages,
  loadingConversations,
  loadingMessages,
  sending,
  uploadingMedia,
  error,
  refreshQinbixinMailbox,
  selectConversation,
  login,
  sendMessage,
  pickMediaPaths,
  uploadMedia,
  uploadError,
} = useQinbixin();

const userName = ref("");
const password = ref("");
const loggingIn = ref(false);
const sendTitle = ref("");
const sendContent = ref("");
const sendFeedback = ref("");

interface PendingMedia {
  kind: "image" | "video" | "file";
  url: string; // empty while uploading
  title: string;
  localPath: string;
  thumb: string | null;
  progress: number;
}

const pendingMedia = ref<PendingMedia[]>([]);

let progressUnlisten: (() => void) | null = null;

async function initProgressListener(): Promise<void> {
  if (progressUnlisten) return;
  progressUnlisten = await listen<{
    index: number;
    fileName: string;
    percent: number;
  }>("qinbixin-upload-progress", (event) => {
    const { fileName, percent } = event.payload;
    const item = pendingMedia.value.find(
      (m) => m.url === "" && m.title === fileName,
    );
    if (item) item.progress = Math.max(0, Math.min(100, percent));
  });
}

const dialogTitle = computed(() => {
  if (!status.value.logged_in) {
    return t("qinbixin.loginTitle");
  }
  return t("qinbixin.mailTitle");
});

const sanitizedMessages = computed(() =>
  messages.value.map((message) => ({
    ...message,
    safeContent: DOMPurify.sanitize(message.content, {
      ALLOWED_TAGS: [
        "p",
        "br",
        "strong",
        "em",
        "b",
        "i",
        "u",
        "s",
        "strike",
        "span",
        "a",
        "img",
        "video",
        "audio",
        "source",
        "ol",
        "ul",
        "li",
        "blockquote",
        "pre",
        "code",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
        "table",
        "thead",
        "tbody",
        "tr",
        "td",
        "th",
      ],
      ALLOWED_ATTR: [
        "href",
        "target",
        "rel",
        "src",
        "alt",
        "type",
        "controls",
        "preload",
        "width",
        "height",
        "style",
        "class",
        "colspan",
        "rowspan",
      ],
      ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|tel:|data:image\/|\/|#)/i,
    }),
    incoming: message.sender_id !== status.value.profile?.id,
  })),
);

function mediaUrls(kind: PendingMedia["kind"]): string[] {
  return pendingMedia.value
    .filter((item) => item.kind === kind && item.url !== "")
    .map((item) => item.url);
}

let mailboxTimer: number | null = null;

function startMailboxPolling(): void {
  if (mailboxTimer !== null) return;
  void refreshQinbixinMailbox();
  mailboxTimer = window.setInterval(() => {
    void refreshQinbixinMailbox();
  }, 5_000);
}

function stopMailboxPolling(): void {
  if (mailboxTimer !== null) {
    window.clearInterval(mailboxTimer);
    mailboxTimer = null;
  }
}

watch(
  () => [props.open, status.value.logged_in] as const,
  ([open, loggedIn]) => {
    if (open && loggedIn) {
      startMailboxPolling();
    } else {
      stopMailboxPolling();
    }
  },
);

onBeforeUnmount(() => {
  stopMailboxPolling();
  progressUnlisten?.();
  progressUnlisten = null;
});

async function submitLogin() {
  if (!userName.value.trim() || !password.value || loggingIn.value) return;
  loggingIn.value = true;
  const ok = await login(userName.value.trim(), password.value);
  loggingIn.value = false;
  if (ok) {
    password.value = "";
  }
}

async function submitMessage() {
  const conversation = selectedConversation.value;
  const media: QinbixinMedia = {
    imageUrls: mediaUrls("image"),
    videoUrls: mediaUrls("video"),
    fileUrls: mediaUrls("file"),
  };
  const hasMedia =
    media.imageUrls.length > 0 ||
    media.videoUrls.length > 0 ||
    media.fileUrls.length > 0;
  if (!conversation) return;
  if (!sendTitle.value.trim()) {
    sendFeedback.value = t("qinbixin.titleRequired");
    return;
  }
  if (!sendContent.value.trim() && !hasMedia) {
    return;
  }
  const result = await sendMessage(
    conversation.id,
    sendTitle.value.trim(),
    sendContent.value,
    media,
  );
  sendFeedback.value = result.success
    ? t("qinbixin.sendSucceeded")
    : result.message || t("qinbixin.sendFailed");
  if (result.success) {
    sendTitle.value = "";
    sendContent.value = "";
    pendingMedia.value = [];
  }
}

async function pickMedia(kind: PendingMedia["kind"]): Promise<void> {
  void initProgressListener();
  const paths = await pickMediaPaths(kind);
  if (paths.length === 0) return;

  const placeholders = await Promise.all(
    paths.map(async (path) => {
      let thumb: string | null = null;
      if (kind !== "file") {
        try {
          thumb = await invoke<string | null>("qinbixin_thumbnail", {
            path,
            kind,
          });
        } catch {
          thumb = null;
        }
      }
      return {
        kind,
        url: "",
        title: path.split(/[\\/]/).pop() || path,
        localPath: path,
        thumb,
        progress: 0,
      };
    }),
  );
  if (kind === "file") {
    pendingMedia.value = pendingMedia.value.filter(
      (item) => item.kind !== "file",
    );
  }
  pendingMedia.value.push(...placeholders);

  const uploadType = kind === "image" ? 0 : kind === "file" ? 1 : 2;
  try {
    const files = await uploadMedia(paths, uploadType);
    // Replace placeholders with actual URLs
    for (let i = 0; i < Math.min(paths.length, files.length); i++) {
      const item = pendingMedia.value.find(
        (m) => m.url === "" && m.localPath === paths[i],
      );
      if (item) {
        item.url = files[i].url;
        item.title = files[i].title;
        item.progress = 100;
      }
    }
  } finally {
    // Remove any placeholders that did not get a URL (failed uploads)
    pendingMedia.value = pendingMedia.value.filter((item) => item.url !== "");
  }
}

function removeMedia(localPath: string): void {
  pendingMedia.value = pendingMedia.value.filter(
    (item) => item.localPath !== localPath,
  );
}

async function openMessageLink(event: MouseEvent) {
  if (!(event.target instanceof Element)) return;
  const link = event.target.closest("a");
  if (!link) return;
  const href = link.getAttribute("href");
  if (!href) return;
  event.preventDefault();
  event.stopPropagation();
  try {
    await openUrl(new URL(href, window.location.href).toString());
  } catch {
    // Browser launch is best-effort; the webview must not navigate instead.
  }
}

async function openExternalUrl(url: string): Promise<void> {
  try {
    await openUrl(new URL(url, window.location.href).toString());
  } catch {
    // Browser launch is best-effort; the webview must not navigate instead.
  }
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
    <form
      v-if="!status.logged_in"
      class="login-form"
      @submit.prevent="submitLogin"
    >
      <label class="field">
        <span>{{ t("qinbixin.userName") }}</span>
        <input
          v-model="userName"
          class="text-input"
          type="text"
          autocomplete="username"
          :placeholder="t('qinbixin.userNamePlaceholder')"
        />
      </label>
      <label class="field">
        <span>{{ t("qinbixin.password") }}</span>
        <input
          v-model="password"
          class="text-input"
          type="password"
          autocomplete="current-password"
        />
      </label>
      <button class="primary login-button" type="submit" :disabled="loggingIn">
        <Loader2 v-if="loggingIn" class="spin" aria-hidden="true" />
        {{ t("qinbixin.login") }}
      </button>
    </form>

    <div v-else class="message-layout">
      <aside class="conversation-list">
        <div v-if="loadingConversations" class="list-state">
          <Loader2 class="spin" aria-hidden="true" />
        </div>
        <p v-else-if="conversations.length === 0" class="list-state">
          {{ t("qinbixin.noConversations") }}
        </p>
        <button
          v-for="conversation in conversations"
          v-else
          :key="conversation.id"
          class="conversation"
          :class="{ selected: selectedConversationId === conversation.id }"
          type="button"
          @click="selectConversation(conversation)"
        >
          <img
            v-if="conversation.avatar"
            class="avatar"
            :src="conversation.avatar"
            alt=""
          />
          <span v-else class="avatar fallback">{{
            conversation.title.slice(0, 1)
          }}</span>
          <span class="conversation-text">
            <span class="conversation-title">{{ conversation.title }}</span>
            <span class="conversation-preview">{{
              conversation.preview || t("qinbixin.noPreview")
            }}</span>
          </span>
          <span v-if="conversation.unread" class="unread-dot" />
        </button>
      </aside>

      <section class="message-panel">
        <div v-if="!selectedConversation" class="list-state">
          {{ t("qinbixin.selectConversation") }}
        </div>
        <template v-else>
          <div class="message-scroll">
            <div v-if="loadingMessages" class="list-state">
              <Loader2 class="spin" aria-hidden="true" />
            </div>
            <article
              v-for="message in sanitizedMessages"
              v-else-if="sanitizedMessages.length > 0"
              :key="message.id"
              class="message"
              :class="{ incoming: message.incoming }"
            >
              <header class="message-header">
                <strong>{{ message.title }}</strong>
                <span class="message-time">{{ message.sent_time }}</span>
              </header>
              <p v-if="message.song_title" class="message-song">
                <span>{{ t("qinbixin.songTitle") }}:</span>
                {{ message.song_title }}
              </p>
              <!-- eslint-disable-next-line vue/no-v-html -->
              <div
                class="message-content"
                @click.capture="openMessageLink"
                v-html="message.safeContent"
              />
              <div v-if="message.tags.length" class="message-tags">
                <span v-for="tag in message.tags" :key="tag"># {{ tag }}</span>
              </div>
              <div v-if="message.images.length" class="media-grid">
                <button
                  v-for="image in message.images"
                  :key="image"
                  class="media-thumb"
                  type="button"
                  :title="t('qinbixin.openExternally')"
                  @click="openExternalUrl(image)"
                >
                  <img :src="image" :alt="message.title" />
                </button>
              </div>
              <div v-if="message.videos.length" class="media-grid">
                <video
                  v-for="video in message.videos"
                  :key="video"
                  controls
                  preload="metadata"
                  :src="video"
                />
              </div>
              <a
                v-if="message.file_url"
                class="attachment-link"
                :href="message.file_url"
                @click.prevent="openExternalUrl(message.file_url)"
              >
                {{ t("qinbixin.attachmentDownload") }}
              </a>
            </article>
            <p v-else class="list-state">{{ t("qinbixin.noMessages") }}</p>
          </div>

          <div class="compose-form">
            <input
              v-model="sendTitle"
              class="text-input"
              type="text"
              :placeholder="t('qinbixin.titlePlaceholder')"
            />
            <textarea
              v-model="sendContent"
              class="text-input content-input"
              :placeholder="t('qinbixin.contentPlaceholder')"
            />
            <div v-if="pendingMedia.length" class="pending-media">
              <div
                v-for="item in pendingMedia"
                :key="item.localPath"
                class="media-chip"
              >
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
                  @click="removeMedia(item.localPath)"
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
                  :disabled="uploadingMedia"
                  @click="pickMedia('image')"
                >
                  <Image aria-hidden="true" />
                </button>
                <button
                  class="icon-button media-button"
                  type="button"
                  :title="t('qinbixin.addVideo')"
                  :disabled="uploadingMedia"
                  @click="pickMedia('video')"
                >
                  <Video aria-hidden="true" />
                </button>
                <button
                  class="icon-button media-button"
                  type="button"
                  :title="t('qinbixin.addAttachment')"
                  :disabled="uploadingMedia"
                  @click="pickMedia('file')"
                >
                  <Paperclip aria-hidden="true" />
                </button>
              </div>
              <span v-if="sendFeedback" class="feedback">{{
                sendFeedback
              }}</span>
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
            </div>
          </div>
        </template>
      </section>
    </div>

    <div v-if="uploadError" class="upload-error">
      <span>{{ uploadError }}</span>
      <button type="button" @click="uploadError = ''">
        <X aria-hidden="true" />
      </button>
    </div>
    <p v-if="error" class="backend-error">{{ error }}</p>
  </BaseModal>
</template>

<style scoped>
.login-form,
.message-layout {
  display: grid;
  gap: 14px;
}

.message-layout {
  grid-template-columns: 220px minmax(0, 1fr);
  min-height: 420px;
  user-select: text;
  -webkit-user-select: text;
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

.login-button,
.send-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 34px;
  padding: 0 16px;
}

.login-button {
  justify-self: start;
}

.conversation-list {
  min-width: 0;
  overflow: auto;
  border-right: 1px solid var(--border-soft);
  padding-right: 10px;
}

.conversation {
  position: relative;
  display: flex;
  width: 100%;
  align-items: center;
  gap: 8px;
  padding: 8px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  text-align: left;
  font: inherit;
  cursor: pointer;
}

.conversation:hover,
.conversation.selected {
  background: var(--bg-hover);
}

.avatar,
.avatar.fallback {
  display: grid;
  flex-shrink: 0;
  width: 30px;
  height: 30px;
  place-items: center;
  border-radius: 50%;
  object-fit: cover;
  background: var(--bg-inset);
  color: var(--text-muted);
  font-size: 12px;
  font-weight: 650;
}

.conversation-text {
  display: grid;
  min-width: 0;
}

.conversation-title {
  overflow: hidden;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.conversation-preview {
  overflow: hidden;
  color: var(--text-muted);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.unread-dot {
  position: absolute;
  top: 8px;
  right: 8px;
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--danger);
}

.message-panel {
  display: flex;
  min-width: 0;
  flex-direction: column;
  gap: 12px;
}

.message-scroll {
  min-height: 0;
  max-height: 300px;
  overflow: auto;
  display: grid;
  gap: 10px;
  padding-right: 4px;
}

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

.message-content {
  color: var(--text-secondary);
  line-height: 1.6;
  overflow-wrap: anywhere;
}

.message-content :deep(p) {
  margin: 0 0 6px;
}

.message-content :deep(p:last-child) {
  margin-bottom: 0;
}

.compose-form {
  display: grid;
  gap: 8px;
}

.content-input {
  height: 110px;
  padding: 8px 10px;
  resize: vertical;
}

.compose-actions {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.media-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-right: auto;
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

.spin {
  width: 16px;
  height: 16px;
  animation: qinbixin-spin 0.8s linear infinite;
}

.list-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 60px;
  color: var(--text-muted);
  font-size: 12px;
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

@keyframes qinbixin-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
