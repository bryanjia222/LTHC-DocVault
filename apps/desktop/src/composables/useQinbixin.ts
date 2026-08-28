import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { ref } from "vue";

import { isTauri } from "./useVault";

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
  sent_time: string;
  images: string[];
  videos: string[];
  file_url: string;
  tags: string[];
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
const loadingConversations = ref(false);
const loadingMessages = ref(false);
const sending = ref(false);
const uploadingMedia = ref(false);
const switchingEnvironment = ref(false);
const switchingAccount = ref(false);
const error = ref("");
const devAccounts = ref<QinbixinDevAccount[]>([]);

let pollTimer: number | null = null;

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
  pollTimer = window.setInterval(() => {
    void refreshQinbixinStatus();
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

async function refreshQinbixinMailbox(): Promise<void> {
  if (!isTauri() || !status.value.logged_in) return;
  await loadConversations(true);
  const selectedId = selectedConversationId.value;
  if (selectedId !== null) {
    await loadMessages(selectedId, true);
  } else if (conversations.value.length) {
    const conversation = conversations.value[0];
    selectedConversationId.value = conversation.id;
    selectedConversation.value = conversation;
    await loadMessages(conversation.id, true);
  }
}

async function selectConversation(
  conversation: QinbixinConversation,
): Promise<void> {
  selectedConversationId.value = conversation.id;
  selectedConversation.value = conversation;
  await loadMessages(conversation.id);
  if (conversation.unread) {
    try {
      await invoke("qinbixin_mark_read", { relationshipId: conversation.id });
      conversation.unread = false;
      if (!conversations.value.some((item) => item.unread)) {
        status.value = { ...status.value, has_unread: false };
      }
    } catch {
      // The message list still loads; the unread flag refreshes on the next poll.
    }
  }
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
  error.value = "";
}

function clearConversationState(): void {
  conversations.value = [];
  selectedConversationId.value = null;
  selectedConversation.value = null;
  messages.value = [];
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
      await loadMessages(relationshipId);
      await refreshQinbixinStatus();
    }
    return result;
  } catch (e) {
    return { success: false, message: String(e) };
  } finally {
    sending.value = false;
  }
}

async function uploadMedia(
  kind: "image" | "video" | "file",
): Promise<QinbixinUploadedFile[]> {
  if (!isTauri() || uploadingMedia.value) return [];
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
  if (paths.length === 0) return [];
  uploadingMedia.value = true;
  try {
    return await invoke<QinbixinUploadedFile[]>("qinbixin_upload", {
      paths,
      uploadType: kind === "image" ? 0 : kind === "file" ? 1 : 2,
    });
  } catch (e) {
    error.value = String(e);
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
    loadingConversations,
    loadingMessages,
    sending,
    uploadingMedia,
    switchingEnvironment,
    switchingAccount,
    error,
    devAccounts,
    refreshQinbixinStatus,
    loadConversations,
    refreshQinbixinMailbox,
    selectConversation,
    login,
    logout,
    sendMessage,
    uploadMedia,
    setEnvironment,
    loadDevAccounts,
    loginDevAccount,
    startPolling,
    stopPolling,
  };
}
