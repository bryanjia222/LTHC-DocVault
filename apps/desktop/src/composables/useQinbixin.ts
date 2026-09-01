import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { computed, ref, watch } from "vue";

import { isTauri } from "./useVault";
import { reportError } from "../utils/reportError";

export interface QinbixinProfile {
  id: number;
  login_name: string;
  nickname: string;
  image_url: string;
}

export interface QinbixinStatus {
  logged_in: boolean;
  profile?: QinbixinProfile;
  has_unread: boolean;
  environment: QinbixinEnvironment;
}

export type QinbixinEnvironment = "production" | "test";

export interface QinbixinDevAccount {
  id: string;
  user_name: string;
}

export interface QinbixinConversation {
  id: number;
  title: string;
  avatar: string;
  is_group: boolean;
  unread: boolean;
  preview: string;
}

export interface QinbixinMessage {
  id: number;
  title: string;
  song_title: string;
  content: string;
  sender_id: number;
  sender_name: string;
  sender_avatar: string;
  conversation_title?: string;
  sent_time: string;
  images: string[];
  videos: string[];
  file_url: string;
  tags: string[];
  comment_count: number;
  relationship_id: number;
}

export interface QinbixinComment {
  id: number;
  member_id: number;
  author: string;
  avatar: string;
  content: string;
  sent_time: string;
  images: string[];
}

export interface QinbixinMedia {
  imageUrls: string[];
  videoUrls: string[];
  fileUrls: string[];
}

export interface QinbixinUploadedFile {
  url: string;
  title: string;
}

const POLL_INTERVAL_MS = 5_000;

const status = ref<QinbixinStatus>({
  logged_in: false,
  has_unread: false,
  environment: "production",
});
const conversations = ref<QinbixinConversation[]>([]);
const selectedConversationId = ref<number | null>(null);
const selectedConversation = ref<QinbixinConversation | null>(null);
const messages = ref<QinbixinMessage[]>([]);
const outboxMessages = ref<QinbixinMessage[]>([]);
const loadingConversations = ref(false);
const loadingMessages = ref(false);
const loadingOutbox = ref(false);
const sending = ref(false);
const uploadingMedia = ref(false);
const switchingEnvironment = ref(false);
const switchingAccount = ref(false);
const error = ref("");
const uploadError = ref("");
const devAccounts = ref<QinbixinDevAccount[]>([]);
const commentsByMessage = ref<Record<number, QinbixinComment[]>>({});
const loadingComments = ref<Record<number, boolean>>({});
const commentError = ref("");
const readWatermarks = ref<Record<string, number>>({});

const COMMENT_WATERMARK_STORAGE_KEY = "docvault-qinbixin-comment-watermarks";
let commentWatermarkScope: string | null = null;

let pollTimer: number | null = null;

function commentWatermarkScopeKey(): string | null {
  if (!status.value.logged_in || !status.value.profile?.id) return null;
  return `${status.value.environment}:${status.value.profile.id}`;
}

function loadCommentWatermarks(): void {
  const scope = commentWatermarkScopeKey();
  if (scope === commentWatermarkScope) return;
  commentWatermarkScope = scope;
  if (!scope) {
    readWatermarks.value = {};
    return;
  }
  try {
    const raw = localStorage.getItem(
      `${COMMENT_WATERMARK_STORAGE_KEY}:${scope}`,
    );
    const parsed = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
    readWatermarks.value = Object.fromEntries(
      Object.entries(parsed)
        .map(([key, value]) => [key, Number(value)] as const)
        .filter(([, value]) => Number.isFinite(value) && value >= 0),
    );
  } catch (error) {
    readWatermarks.value = {};
    reportError("qinbixin.commentWatermarks.read", error);
  }
}

function saveCommentWatermarks(): void {
  if (!commentWatermarkScope) return;
  try {
    localStorage.setItem(
      `${COMMENT_WATERMARK_STORAGE_KEY}:${commentWatermarkScope}`,
      JSON.stringify(readWatermarks.value),
    );
  } catch (error) {
    // Local notification state is best-effort and must not break mailbox loading.
    reportError("qinbixin.commentWatermarks.persist", error);
  }
}

function initializeCommentWatermarks(items: QinbixinMessage[]): void {
  if (!commentWatermarkScope || items.length === 0) return;
  const current = readWatermarks.value;
  let changed = false;
  for (const message of items) {
    const key = String(message.id);
    if (!(key in current)) {
      current[key] = message.comment_count;
      changed = true;
    }
  }
  if (changed) {
    readWatermarks.value = { ...current };
    saveCommentWatermarks();
  }
}

function syncOutboxCommentCounts(): void {
  if (outboxMessages.value.length === 0 || messages.value.length === 0) return;
  const inboxCounts = new Map(
    messages.value.map((message) => [message.id, message.comment_count]),
  );
  let changed = false;
  const next = outboxMessages.value.map((message) => {
    const count = inboxCounts.get(message.id);
    if (count === undefined || count === message.comment_count) return message;
    changed = true;
    return { ...message, comment_count: count };
  });
  if (changed) {
    outboxMessages.value = next;
  }
}

function unreadCommentCount(message: QinbixinMessage): number {
  const watermark = readWatermarks.value[String(message.id)] ?? 0;
  return Math.max(0, message.comment_count - watermark);
}

function markMessageCommentsRead(message: QinbixinMessage): void {
  if (!commentWatermarkScope) return;
  const key = String(message.id);
  const previous = readWatermarks.value[key] ?? 0;
  const next = Math.max(previous, message.comment_count);
  if (previous === next) return;
  readWatermarks.value = { ...readWatermarks.value, [key]: next };
  saveCommentWatermarks();
}

function markAllMessagesCommentsRead(): void {
  if (!commentWatermarkScope) return;
  const next = { ...readWatermarks.value };
  for (const message of [...messages.value, ...outboxMessages.value]) {
    next[String(message.id)] = message.comment_count;
  }
  readWatermarks.value = next;
  saveCommentWatermarks();
}

watch(status, loadCommentWatermarks, { immediate: true });

const hasQinbixinUnread = computed(
  () =>
    status.value.has_unread ||
    messages.value.some((message) => unreadCommentCount(message) > 0) ||
    outboxMessages.value.some((message) => unreadCommentCount(message) > 0),
);

function statusEquals(left: QinbixinStatus, right: QinbixinStatus): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

export async function refreshQinbixinStatus(): Promise<void> {
  if (!isTauri()) return;
  try {
    const next = await invoke<QinbixinStatus>("qinbixin_status");
    if (!statusEquals(status.value, next)) {
      status.value = next;
    }
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}

function startPolling(): void {
  if (pollTimer !== null || !isTauri()) return;
  void refreshQinbixinStatus();
  void refreshQinbixinMailbox();
  pollTimer = window.setInterval(() => {
    void refreshQinbixinStatus();
    void refreshQinbixinMailbox();
  }, POLL_INTERVAL_MS);
}

function stopPolling(): void {
  if (pollTimer !== null) {
    window.clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function loadConversations(background = false): Promise<void> {
  if (!isTauri()) return;
  if (!background) {
    loadingConversations.value = true;
  }
  try {
    const next = await invoke<QinbixinConversation[]>("qinbixin_conversations");
    if (JSON.stringify(conversations.value) !== JSON.stringify(next)) {
      conversations.value = next;
      const selectedId = selectedConversationId.value;
      if (selectedId !== null) {
        selectedConversation.value =
          next.find((conversation) => conversation.id === selectedId) ?? null;
      }
    }
    const hasUnread = next.some((conversation) => conversation.unread);
    if (status.value.has_unread !== hasUnread) {
      status.value = { ...status.value, has_unread: hasUnread };
    }
    error.value = "";
  } catch (e) {
    if (String(e).includes("AUTH_EXPIRED")) {
      status.value = {
        ...status.value,
        logged_in: false,
        has_unread: false,
      };
    }
    error.value = String(e);
  } finally {
    if (!background) {
      loadingConversations.value = false;
    }
  }
}

async function loadMessages(
  relationshipId: number,
  background = false,
): Promise<void> {
  if (!isTauri()) return;
  if (!background) {
    loadingMessages.value = true;
  }
  try {
    const next = await invoke<QinbixinMessage[]>("qinbixin_messages", {
      relationshipId,
    });
    initializeCommentWatermarks(next);
    if (JSON.stringify(messages.value) !== JSON.stringify(next)) {
      messages.value = next;
    }
    error.value = "";
  } catch (e) {
    if (String(e).includes("AUTH_EXPIRED")) {
      status.value = {
        ...status.value,
        logged_in: false,
        has_unread: false,
      };
    }
    error.value = String(e);
  } finally {
    if (!background) {
      loadingMessages.value = false;
    }
  }
}

async function refreshQinbixinMailbox(markRead = false): Promise<void> {
  if (!isTauri() || !status.value.logged_in) return;
  await loadConversations(true);
  await loadInbox(true);
  await loadOutbox(true);
  if (markRead) {
    for (const conversation of conversations.value.filter(
      (item) => item.unread,
    )) {
      await markConversationRead(conversation);
    }
  }
}

async function loadOutbox(background = false): Promise<void> {
  if (!isTauri()) return;
  if (!background) {
    loadingOutbox.value = true;
  }
  try {
    const next = await invoke<QinbixinMessage[]>("qinbixin_outbox");
    initializeCommentWatermarks(next);
    if (JSON.stringify(outboxMessages.value) !== JSON.stringify(next)) {
      outboxMessages.value = next;
    }
    syncOutboxCommentCounts();
    error.value = "";
  } catch (e) {
    if (String(e).includes("AUTH_EXPIRED")) {
      status.value = {
        ...status.value,
        logged_in: false,
        has_unread: false,
      };
    }
    error.value = String(e);
  } finally {
    if (!background) {
      loadingOutbox.value = false;
    }
  }
}

async function selectConversation(
  conversation: QinbixinConversation,
): Promise<void> {
  if (selectedConversationId.value === conversation.id) {
    selectedConversation.value = conversation;
    if (conversation.unread) {
      await markConversationRead(conversation);
    }
    return;
  }
  selectedConversationId.value = conversation.id;
  selectedConversation.value = conversation;
  await loadMessages(conversation.id);
  if (conversation.unread) {
    await markConversationRead(conversation);
  }
}

async function loadInbox(background = false): Promise<void> {
  if (!isTauri()) return;
  if (!background) {
    loadingMessages.value = true;
  }
  try {
    const next = await invoke<QinbixinMessage[]>("qinbixin_inbox");
    initializeCommentWatermarks(next);
    if (JSON.stringify(messages.value) !== JSON.stringify(next)) {
      messages.value = next;
    }
    syncOutboxCommentCounts();
    error.value = "";
  } catch (e) {
    if (String(e).includes("AUTH_EXPIRED")) {
      status.value = {
        ...status.value,
        logged_in: false,
        has_unread: false,
      };
    }
    error.value = String(e);
  } finally {
    if (!background) {
      loadingMessages.value = false;
    }
  }
}

async function markConversationRead(
  conversation: QinbixinConversation,
): Promise<void> {
  try {
    await invoke("qinbixin_mark_read", { relationshipId: conversation.id });
    conversation.unread = false;
    if (!conversations.value.some((item) => item.unread)) {
      status.value = { ...status.value, has_unread: false };
    }
  } catch {
    // The unread flag refreshes on the next poll.
  }
}

async function markAllQinbixinRead(): Promise<void> {
  if (!isTauri() || !status.value.logged_in) return;
  for (const conversation of conversations.value.filter(
    (item) => item.unread,
  )) {
    await markConversationRead(conversation);
  }
  markAllMessagesCommentsRead();
}

async function loadMessageComments(
  messageId: number,
): Promise<QinbixinComment[]> {
  if (!isTauri()) return [];
  loadingComments.value = { ...loadingComments.value, [messageId]: true };
  commentError.value = "";
  try {
    const comments = await invoke<QinbixinComment[]>(
      "qinbixin_message_comments",
      { messageId },
    );
    commentsByMessage.value = {
      ...commentsByMessage.value,
      [messageId]: comments,
    };
    return comments;
  } catch (e) {
    commentError.value = String(e);
    return [];
  } finally {
    loadingComments.value = { ...loadingComments.value, [messageId]: false };
  }
}

async function addComment(
  messageId: number,
  content: string,
): Promise<{ success: boolean; message: string }> {
  if (!isTauri() || !content.trim()) {
    return { success: false, message: "" };
  }
  commentError.value = "";
  try {
    const result = await invoke<{ success: boolean; message: string }>(
      "qinbixin_add_comment",
      {
        messageId,
        content: content.trim(),
        imageUrls: [],
      },
    );
    if (result.success) {
      const updateCount = (message: QinbixinMessage) => {
        if (message.id !== messageId) return;
        message.comment_count += 1;
        markMessageCommentsRead(message);
      };
      messages.value.forEach(updateCount);
      outboxMessages.value.forEach(updateCount);
      await loadMessageComments(messageId);
    } else {
      commentError.value = result.message || "request failed";
    }
    return result;
  } catch (e) {
    commentError.value = String(e);
    return { success: false, message: String(e) };
  }
}

function clearCommentError(): void {
  commentError.value = "";
}

async function login(userName: string, password: string): Promise<boolean> {
  try {
    status.value = await invoke<QinbixinStatus>("qinbixin_login", {
      userName,
      password,
    });
    error.value = "";
    await loadConversations();
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  }
}

async function logout(): Promise<void> {
  if (!isTauri()) return;
  await invoke("qinbixin_logout");
  status.value = {
    ...status.value,
    logged_in: false,
    has_unread: false,
  };
  conversations.value = [];
  selectedConversationId.value = null;
  selectedConversation.value = null;
  messages.value = [];
  outboxMessages.value = [];
  commentsByMessage.value = {};
  loadingComments.value = {};
  error.value = "";
}

function clearConversationState(): void {
  conversations.value = [];
  selectedConversationId.value = null;
  selectedConversation.value = null;
  messages.value = [];
  outboxMessages.value = [];
  commentsByMessage.value = {};
  loadingComments.value = {};
}

async function setEnvironment(environment: QinbixinEnvironment): Promise<void> {
  if (!isTauri() || !import.meta.env.DEV || switchingEnvironment.value) return;
  switchingEnvironment.value = true;
  try {
    status.value = await invoke<QinbixinStatus>("qinbixin_set_environment", {
      environment,
    });
    error.value = "";
    clearConversationState();
  } catch (e) {
    error.value = String(e);
  } finally {
    switchingEnvironment.value = false;
  }
}

async function loadDevAccounts(): Promise<void> {
  if (!isTauri() || !import.meta.env.DEV) return;
  try {
    devAccounts.value = await invoke<QinbixinDevAccount[]>(
      "qinbixin_dev_accounts",
    );
  } catch (e) {
    devAccounts.value = [];
    error.value = String(e);
  }
}

async function loginDevAccount(accountId: string): Promise<boolean> {
  if (
    !isTauri() ||
    !import.meta.env.DEV ||
    !accountId ||
    switchingAccount.value
  ) {
    return false;
  }
  const currentEnvironment = status.value.environment;
  if (currentEnvironment !== "test") {
    await setEnvironment("test");
    if (status.value.environment !== "test") return false;
  }
  switchingAccount.value = true;
  try {
    status.value = await invoke<QinbixinStatus>("qinbixin_login_dev_account", {
      accountId,
    });
    error.value = "";
    clearConversationState();
    await loadConversations();
    return true;
  } catch (e) {
    error.value = String(e);
    return false;
  } finally {
    switchingAccount.value = false;
  }
}

async function sendMessage(
  relationshipId: number,
  title: string,
  content: string,
  media: QinbixinMedia = { imageUrls: [], videoUrls: [], fileUrls: [] },
): Promise<{ success: boolean; message: string }> {
  if (!isTauri()) return { success: true, message: "" };
  sending.value = true;
  try {
    const result = await invoke<{ success: boolean; message: string }>(
      "qinbixin_send",
      {
        relationshipId,
        title,
        content,
        media,
      },
    );
    if (result.success) {
      await loadInbox();
      await loadOutbox();
      await refreshQinbixinStatus();
    }
    return result;
  } catch (e) {
    return { success: false, message: String(e) };
  } finally {
    sending.value = false;
  }
}

async function pickMediaPaths(
  kind: "image" | "video" | "file",
): Promise<string[]> {
  const filters =
    kind === "image"
      ? [
          {
            name: "图片",
            extensions: ["jpg", "jpeg", "png", "gif", "webp", "tiff", "tif"],
          },
        ]
      : kind === "video"
        ? [{ name: "视频", extensions: ["mp4", "webm", "ogg", "mov"] }]
        : [
            {
              name: "文件",
              extensions: [
                "pdf",
                "txt",
                "zip",
                "rar",
                "7z",
                "doc",
                "docx",
                "xls",
                "xlsx",
                "ppt",
                "pptx",
                "mp3",
                "mp4",
              ],
            },
          ];
  const selected = await open({ multiple: kind !== "file", filters });
  const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
  return paths;
}

async function uploadMedia(
  paths: string[],
  uploadType: number,
): Promise<QinbixinUploadedFile[]> {
  if (!isTauri() || uploadingMedia.value || paths.length === 0) return [];
  uploadError.value = "";
  uploadingMedia.value = true;
  try {
    return await invoke<QinbixinUploadedFile[]>("qinbixin_upload", {
      paths,
      uploadType,
    });
  } catch (e) {
    uploadError.value = String(e);
    return [];
  } finally {
    uploadingMedia.value = false;
  }
}

export function useQinbixin() {
  return {
    status,
    conversations,
    selectedConversationId,
    selectedConversation,
    messages,
    outboxMessages,
    loadingConversations,
    loadingMessages,
    loadingOutbox,
    sending,
    uploadingMedia,
    uploadError,
    switchingEnvironment,
    switchingAccount,
    error,
    devAccounts,
    refreshQinbixinStatus,
    loadConversations,
    refreshQinbixinMailbox,
    loadInbox,
    loadOutbox,
    selectConversation,
    markAllQinbixinRead,
    commentsByMessage,
    loadingComments,
    commentError,
    clearCommentError,
    loadMessageComments,
    addComment,
    markMessageCommentsRead,
    unreadCommentCount,
    hasQinbixinUnread,
    login,
    logout,
    sendMessage,
    uploadMedia,
    pickMediaPaths,
    setEnvironment,
    loadDevAccounts,
    loginDevAccount,
    startPolling,
    stopPolling,
  };
}
