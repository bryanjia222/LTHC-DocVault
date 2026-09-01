import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onBeforeUnmount, ref, watch, type Ref } from "vue";
import { useI18n } from "vue-i18n";

import { reportError } from "../utils/reportError";
import { useQinbixin, type QinbixinMedia } from "./useQinbixin";
import type { QinbixinView } from "./useQinbixinMailbox";
import { sanitizeQinbixinRichContent } from "../components/qinbixin/content";

export interface QinbixinPendingMedia {
  kind: "image" | "video" | "file";
  url: string;
  title: string;
  localPath: string;
  thumb: string | null;
  progress: number;
}

export function useQinbixinCompose(activeView: Ref<QinbixinView>) {
  const { t } = useI18n();
  const {
    conversations,
    selectedConversationId,
    sending,
    uploadingMedia,
    sendMessage,
    pickMediaPaths,
    uploadMedia,
  } = useQinbixin();

  const sendTitle = ref("");
  const sendContent = ref("");
  const sendFeedback = ref("");
  const sendRecipientId = ref<number | null>(null);
  const pendingMedia = ref<QinbixinPendingMedia[]>([]);

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
        (media) => media.url === "" && media.title === fileName,
      );
      if (item) item.progress = Math.max(0, Math.min(100, percent));
    });
  }

  function mediaUrls(kind: QinbixinPendingMedia["kind"]): string[] {
    return pendingMedia.value
      .filter((item) => item.kind === kind && item.url !== "")
      .map((item) => item.url);
  }

  function clear(): void {
    sendTitle.value = "";
    sendContent.value = "";
    sendFeedback.value = "";
    pendingMedia.value = [];
  }

  async function submitMessage() {
    const media: QinbixinMedia = {
      imageUrls: mediaUrls("image"),
      videoUrls: mediaUrls("video"),
      fileUrls: mediaUrls("file"),
    };
    const recipientId = sendRecipientId.value;
    if (!recipientId) {
      const message = t("qinbixin.recipientRequired");
      sendFeedback.value = message;
      reportError("qinbixin.compose.recipient", new Error(message));
      return;
    }
    if (!sendTitle.value.trim()) {
      const message = t("qinbixin.titleRequired");
      sendFeedback.value = message;
      reportError("qinbixin.compose.title", new Error(message));
      return;
    }

    const result = await sendMessage(
      recipientId,
      sendTitle.value.trim(),
      sanitizeQinbixinRichContent(sendContent.value),
      media,
    );
    sendFeedback.value = result.success
      ? t("qinbixin.sendSucceeded")
      : result.message || t("qinbixin.sendFailed");
    if (result.success) clear();
  }

  async function pickMedia(kind: QinbixinPendingMedia["kind"]): Promise<void> {
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
      for (
        let index = 0;
        index < Math.min(paths.length, files.length);
        index++
      ) {
        const item = pendingMedia.value.find(
          (media) => media.url === "" && media.localPath === paths[index],
        );
        if (item) {
          item.url = files[index].url;
          item.title = files[index].title;
          item.progress = 100;
        }
      }
    } finally {
      pendingMedia.value = pendingMedia.value.filter((item) => item.url !== "");
    }
  }

  function removeMedia(localPath: string): void {
    pendingMedia.value = pendingMedia.value.filter(
      (item) => item.localPath !== localPath,
    );
  }

  watch(
    activeView,
    (view) => {
      if (view === "compose" && sendRecipientId.value === null) {
        sendRecipientId.value = selectedConversationId.value;
      }
    },
    { immediate: true },
  );

  onBeforeUnmount(() => {
    progressUnlisten?.();
    progressUnlisten = null;
  });

  return {
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
    clear,
  };
}

export type QinbixinComposeController = ReturnType<typeof useQinbixinCompose>;
