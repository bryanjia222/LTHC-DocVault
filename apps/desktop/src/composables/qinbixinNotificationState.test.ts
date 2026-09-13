import { beforeEach, describe, expect, it } from "vitest";

import type { QinbixinConversation } from "./useQinbixin";
import {
  resetQinbixinNotificationState,
  updateQinbixinNotificationState,
} from "./qinbixinNotificationState";

function conversation(id: number, unread: boolean): QinbixinConversation {
  return {
    id,
    unread,
    title: `chat-${id}`,
    avatar: "",
    is_group: false,
    preview: "",
  };
}

describe("qinbixinNotificationState", () => {
  beforeEach(resetQinbixinNotificationState);

  it("uses the first account snapshot as a silent baseline", () => {
    expect(
      updateQinbixinNotificationState(
        "production:1",
        [conversation(1, true)],
        new Map([[10, 2]]),
      ),
    ).toEqual({ newMessages: 0, newReplies: 0 });
  });

  it("counts newly unread conversations and reply increments", () => {
    updateQinbixinNotificationState(
      "production:1",
      [conversation(1, false)],
      new Map([[10, 0]]),
    );
    expect(
      updateQinbixinNotificationState(
        "production:1",
        [conversation(1, true)],
        new Map([[10, 3]]),
      ),
    ).toEqual({ newMessages: 1, newReplies: 3 });
  });

  it("does not repeat unchanged unread state", () => {
    updateQinbixinNotificationState(
      "production:1",
      [conversation(1, true)],
      new Map([[10, 2]]),
    );
    expect(
      updateQinbixinNotificationState(
        "production:1",
        [conversation(1, true)],
        new Map([[10, 2]]),
      ),
    ).toEqual({ newMessages: 0, newReplies: 0 });
  });

  it("starts a new silent baseline when the account changes", () => {
    updateQinbixinNotificationState("production:1", [], new Map());
    expect(
      updateQinbixinNotificationState(
        "production:2",
        [conversation(2, true)],
        new Map([[20, 1]]),
      ),
    ).toEqual({ newMessages: 0, newReplies: 0 });
  });
});
