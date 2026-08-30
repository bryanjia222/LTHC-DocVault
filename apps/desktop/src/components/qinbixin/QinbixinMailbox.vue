<script setup lang="ts">
import { Loader2 } from "@lucide/vue";
import Viewer from "viewerjs";
import "viewerjs/dist/viewer.css";
import { useI18n } from "vue-i18n";

import QinbixinMessageCard from "../QinbixinMessageCard.vue";
import { openUrl } from "../../composables/useVault";
import type {
  QinbixinMailboxController,
} from "../../composables/useQinbixinMailbox";

const props = defineProps<{ mailbox: QinbixinMailboxController }>();
const { t } = useI18n();

const {
  activeView,
  inboxMessages,
  outgoingMessages,
  loadingMessages,
  loadingOutbox,
  outboxParticipant,
} = props.mailbox;

function openImagePreview(url: string): void {
  const container = document.createElement("div");
  container.style.display = "none";
  const image = document.createElement("img");
  image.src = url;
  container.appendChild(image);
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
  <section v-if="activeView === 'inbox'" class="message-panel">
    <div v-if="loadingMessages" class="list-state">
      <Loader2 class="spin" aria-hidden="true" />
    </div>
    <div v-else-if="inboxMessages.length" class="message-scroll">
      <QinbixinMessageCard
        v-for="message in inboxMessages"
        :key="message.id"
        :message="message"
        :incoming="true"
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
    <div v-else-if="outgoingMessages.length" class="message-scroll">
      <QinbixinMessageCard
        v-for="message in outgoingMessages"
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
</template>

<style scoped>
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

.message-scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
  display: grid;
  gap: 10px;
  padding-right: 4px;
}

.spin {
  width: 16px;
  height: 16px;
  animation: qinbixin-mailbox-spin 0.8s linear infinite;
}

.list-state {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 60px;
  color: var(--text-muted);
  font-size: 12px;
}

@keyframes qinbixin-mailbox-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
