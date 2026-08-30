import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useQinbixin, type QinbixinMessage } from "./useQinbixin";
import { sanitizeQinbixinMessage } from "../components/qinbixin/content";

export type QinbixinView = "inbox" | "outbox" | "compose";

interface QinbixinMailboxSource {
  open: boolean;
  initialView?: QinbixinView;
}

export function useQinbixinMailbox(source: QinbixinMailboxSource) {
  const { t } = useI18n();
  const {
    status,
    selectedConversationId,
    messages,
    outboxMessages,
    loadingMessages,
    loadingOutbox,
    refreshQinbixinMailbox,
    loadOutbox,
  } = useQinbixin();

  const activeView = ref<QinbixinView>("inbox");
  let mailboxTimer: number | null = null;

  const dialogTitle = computed(() => {
    if (!status.value.logged_in) return t("qinbixin.loginTitle");
    const keys: Record<QinbixinView, string> = {
      inbox: "qinbixin.inboxTitle",
      outbox: "qinbixin.outboxTitle",
      compose: "qinbixin.composeTitle",
    };
    return t(keys[activeView.value]);
  });

  const inboxMessages = computed(() =>
    messages.value
      .filter((message) => message.sender_id !== status.value.profile?.id)
      .map(sanitizeQinbixinMessage),
  );

  const outgoingMessages = computed(() =>
    outboxMessages.value.map(sanitizeQinbixinMessage),
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

  function outboxParticipant(message: QinbixinMessage): string {
    return (
      inboxConversationById.value.get(message.id) ||
      t("qinbixin.unknownRecipient")
    );
  }

  function startMailboxPolling(): void {
    if (mailboxTimer !== null) return;
    void refreshQinbixinMailbox(activeView.value === "inbox");
    mailboxTimer = window.setInterval(() => {
      void refreshQinbixinMailbox(activeView.value === "inbox");
    }, 5_000);
  }

  function stopMailboxPolling(): void {
    if (mailboxTimer === null) return;
    window.clearInterval(mailboxTimer);
    mailboxTimer = null;
  }

  function setActiveView(view: QinbixinView): void {
    activeView.value = view;
  }

  watch(
    () => [source.open, status.value.logged_in] as const,
    ([open, loggedIn]) => {
      if (open && loggedIn) {
        activeView.value = source.initialView ?? "inbox";
        startMailboxPolling();
      } else {
        stopMailboxPolling();
      }
    },
  );

  watch(activeView, (view) => {
    if (!status.value.logged_in) return;
    if (view === "inbox") void refreshQinbixinMailbox(true);
    if (view === "outbox") void loadOutbox();
  });

  return {
    activeView,
    dialogTitle,
    loadingMessages,
    loadingOutbox,
    inboxMessages,
    outgoingMessages,
    selectedConversationId,
    outboxParticipant,
    setActiveView,
  };
}

export type QinbixinMailboxController = ReturnType<typeof useQinbixinMailbox>;
