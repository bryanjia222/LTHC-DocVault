<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import DOMPurify from "dompurify";
import Viewer from "viewerjs";
import "viewerjs/dist/viewer.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { File, Image, Loader2, Paperclip, Send, Video, X } from "@lucide/vue";

import BaseModal from "./BaseModal.vue";
import QinbixinMessageCard from "./QinbixinMessageCard.vue";
import RichTextEditor from "./RichTextEditor.vue";
import { openUrl } from "../composables/useVault";
import { useQinbixin, type QinbixinMedia } from "../composables/useQinbixin";

type QinbixinView = "inbox" | "outbox" | "compose";

const props = defineProps<{
  open: boolean;
  initialView?: QinbixinView;
}>();

const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();

const {
  status,
  conversations,
  selectedConversationId,
  messages,
  outboxMessages,
  loadingMessages,
  loadingOutbox,
  sending,
  uploadingMedia,
  error,
  refreshQinbixinMailbox,
  loadOutbox,
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
const activeView = ref<QinbixinView>("inbox");
const sendRecipientId = ref<number | null>(null);

interface PendingMedia {
  kind: "image" | "video" | "file";
  url: string; // empty while uploading
  title: string;
  localPath: string;
  thumb: string | null;
  progress: number;
}

const pendingMedia = ref<PendingMedia[]>([]);

const SANITIZE_CONFIG = {
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
    "sub",
    "sup",
    "figure",
    "figcaption",
    "caption",
    "colgroup",
    "col",
    "tfoot",
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
    "id",
    "name",
    "start",
    "dir",
    "lang",
    "span",
    "poster",
    "srcset",
    "sizes",
    "datetime",
    "cite",
    "data-pagebreak",
  ],
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|tel:|data:image\/|\/|#)/i,
};

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
  const keys: Record<QinbixinView, string> = {
    inbox: "qinbixin.inboxTitle",
    outbox: "qinbixin.outboxTitle",
    compose: "qinbixin.composeTitle",
  };
  return t(keys[activeView.value]);
});

const sanitizedMessages = computed(() =>
  messages.value
    .filter((message) => message.sender_id !== status.value.profile?.id)
    .map((message) => ({
      ...message,
      safeContent: DOMPurify.sanitize(message.content, SANITIZE_CONFIG),
      incoming: true,
    })),
);

const sanitizedOutbox = computed(() =>
  outboxMessages.value.map((message) => ({
    ...message,
    safeContent: DOMPurify.sanitize(message.content, SANITIZE_CONFIG),
  })),
);

const inboxConversationById = computed(() => {
  const titles = new Map<number, string>();
  for (const message of messages.value) {
    if (message.conversation_title) {
      titles.set(message.id, message.conversation_title);
    }
  }
  return titles;
});

function outboxParticipant(message: { id: number }): string {
  return (
    inboxConversationById.value.get(message.id) ||
    t("qinbixin.unknownRecipient")
  );
}

function mediaUrls(kind: PendingMedia["kind"]): string[] {
  return pendingMedia.value
    .filter((item) => item.kind === kind && item.url !== "")
    .map((item) => item.url);
}

function sanitizeRichContent(html: string): string {
  const content = DOMPurify.sanitize(html, SANITIZE_CONFIG).trim();
  const emptyHtml = /^(?:<p>(?:\s|&nbsp;|<br\s*\/?>)*<\/p>|<br\s*\/?>)+$/i.test(
    content,
  );
  return emptyHtml ? "" : content;
}

let mailboxTimer: number | null = null;

function startMailboxPolling(): void {
  if (mailboxTimer !== null) return;
  void refreshQinbixinMailbox(activeView.value === "inbox");
  mailboxTimer = window.setInterval(() => {
    void refreshQinbixinMailbox(activeView.value === "inbox");
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
      activeView.value = props.initialView ?? "inbox";
      if (activeView.value === "compose") {
        sendRecipientId.value = selectedConversationId.value;
      }
      startMailboxPolling();
    } else {
      stopMailboxPolling();
    }
  },
);

watch(activeView, (view) => {
  if (view === "inbox" && status.value.logged_in) {
    void refreshQinbixinMailbox(true);
  }
  if (view === "compose" && sendRecipientId.value === null) {
    sendRecipientId.value = selectedConversationId.value;
  }
  if (view === "outbox" && status.value.logged_in) {
    void loadOutbox();
  }
});

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
  const media: QinbixinMedia = {
    imageUrls: mediaUrls("image"),
    videoUrls: mediaUrls("video"),
    fileUrls: mediaUrls("file"),
  };
  const recipientId = sendRecipientId.value;
  if (!recipientId) {
    sendFeedback.value = t("qinbixin.recipientRequired");
    return;
  }
  if (!sendTitle.value.trim()) {
    sendFeedback.value = t("qinbixin.titleRequired");
    return;
  }
  const result = await sendMessage(
    recipientId,
    sendTitle.value.trim(),
    sanitizeRichContent(sendContent.value),
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

function openImagePreview(url: string): void {
  const container = document.createElement("div");
  container.style.display = "none";
  const img = document.createElement("img");
  img.src = url;
  container.appendChild(img);
  document.body.appendChild(container);
  const viewer = new Viewer(container, {
    navbar: false,
    toolbar: true,
    hide: () => {
      viewer.destroy();
      container.remove();
    },
  });
  viewer.show();
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

    <div v-else class="mail-root">
      <nav class="mail-tabs">
        <button
          type="button"
          :class="{ active: activeView === 'inbox' }"
          @click="activeView = 'inbox'"
        >
          {{ t("qinbixin.inboxTab") }}
        </button>
        <button
          type="button"
          :class="{ active: activeView === 'outbox' }"
          @click="activeView = 'outbox'"
        >
          {{ t("qinbixin.outboxTab") }}
        </button>
        <button
          type="button"
          :class="{ active: activeView === 'compose' }"
          @click="activeView = 'compose'"
        >
          {{ t("qinbixin.composeTab") }}
        </button>
      </nav>

      <section v-if="activeView === 'inbox'" class="message-panel">
        <div v-if="loadingMessages" class="list-state">
          <Loader2 class="spin" aria-hidden="true" />
        </div>
        <div v-else-if="sanitizedMessages.length" class="message-scroll">
          <QinbixinMessageCard
            v-for="message in sanitizedMessages"
            :key="message.id"
            :message="message"
            :incoming="message.incoming"
            :participant="message.sender_name || t('qinbixin.unknownSender')"
            @preview-image="openImagePreview"
            @open-link="openMessageLink"
            @open-external="openExternalUrl"
          />
        </div>
        <p v-else class="list-state">{{ t("qinbixin.noMessages") }}</p>
      </section>

      <section v-else-if="activeView === 'outbox'" class="outbox-panel">
        <div v-if="loadingOutbox" class="list-state">
          <Loader2 class="spin" aria-hidden="true" />
        </div>
        <div v-else-if="sanitizedOutbox.length" class="message-scroll">
          <QinbixinMessageCard
            v-for="message in sanitizedOutbox"
            :key="message.id"
            :message="message"
            :incoming="false"
            :participant="outboxParticipant(message)"
            @preview-image="openImagePreview"
            @open-link="openMessageLink"
            @open-external="openExternalUrl"
          />
        </div>
        <p v-else class="list-state">{{ t("qinbixin.noMessages") }}</p>
      </section>

      <form v-else class="compose-view" @submit.prevent="submitMessage">
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
          </div>
        </div>
      </form>
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
.login-form {
  display: grid;
  gap: 14px;
}

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

.message-panel {
  flex: 1;
  display: flex;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 12px;
  user-select: text;
  -webkit-user-select: text;
}

.message-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: grid;
  gap: 10px;
  padding-right: 4px;
}

.compose-form {
  display: grid;
  gap: 8px;
}

.outbox-panel {
  flex: 1;
  display: flex;
  min-width: 0;
  min-height: 0;
  flex-direction: column;
  gap: 10px;
  user-select: text;
  -webkit-user-select: text;
}

.compose-view {
  flex: 1;
  min-height: 0;
  overflow: auto;
  user-select: text;
  -webkit-user-select: text;
}

.rich-editor {
  min-width: 0;
}

.rich-editor :deep(.tox-tinymce) {
  border-color: var(--border-strong);
  border-radius: var(--radius-sm);
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
