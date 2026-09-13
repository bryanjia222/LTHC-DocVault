import type { QinbixinConversation } from "./useQinbixin";

export interface QinbixinNotificationChange {
  newMessages: number;
  newReplies: number;
}

let activeScope: string | null = null;
let initialized = false;
let unreadConversationIds = new Set<number>();
let unreadRepliesByMessage = new Map<number, number>();

/**
 * Compare the latest mailbox state with the last successful poll. The first
 * snapshot for an account is only a baseline, so launching the app does not
 * re-notify every item that was already unread.
 */
export function updateQinbixinNotificationState(
  scope: string,
  conversations: QinbixinConversation[],
  unreadReplies: ReadonlyMap<number, number>,
): QinbixinNotificationChange {
  if (activeScope !== scope) {
    activeScope = scope;
    initialized = false;
  }

  const nextUnreadConversationIds = new Set(
    conversations.filter((item) => item.unread).map((item) => item.id),
  );
  const nextUnreadReplies = new Map(unreadReplies);

  let newMessages = 0;
  let newReplies = 0;
  if (initialized) {
    newMessages = [...nextUnreadConversationIds].filter(
      (id) => !unreadConversationIds.has(id),
    ).length;
    for (const [messageId, count] of nextUnreadReplies) {
      newReplies += Math.max(
        0,
        count - (unreadRepliesByMessage.get(messageId) ?? 0),
      );
    }
  }

  initialized = true;
  unreadConversationIds = nextUnreadConversationIds;
  unreadRepliesByMessage = nextUnreadReplies;
  return { newMessages, newReplies };
}

export function resetQinbixinNotificationState(): void {
  activeScope = null;
  initialized = false;
  unreadConversationIds = new Set();
  unreadRepliesByMessage = new Map();
}
