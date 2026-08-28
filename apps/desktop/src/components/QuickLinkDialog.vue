<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Loader2, RefreshCw } from "@lucide/vue";
import BaseModal from "./BaseModal.vue";
import { useVault } from "../composables/useVault";
import type { QuickLink } from "../composables/useQuickLinks";

/*
 * Add / edit a sidebar quick link. The URL field auto-fetches the page's title
 * + favicon (best-effort; a failed fetch leaves the title as the raw URL and no
 * icon), and the title stays editable so the user can customize it. "Edit" mode
 * pre-fills from the existing link and can re-fetch after changing the URL.
 */

const props = defineProps<{
  open: boolean;
  mode: "add" | "edit";
  link?: QuickLink;
}>();

const emit = defineEmits<{
  close: [];
  save: [payload: { title: string; url: string; favicon?: string }];
}>();

const { t } = useI18n();
const { fetchUrlMeta } = useVault();

const url = ref("");
const title = ref("");
const favicon = ref<string | undefined>(undefined);
const fetching = ref(false);
const urlInput = ref<HTMLInputElement | null>(null);

// Reset the form each time the dialog opens.
watch(
  () => props.open,
  (open) => {
    if (!open) return;
    url.value = props.link?.url ?? "";
    title.value = props.link?.title ?? "";
    favicon.value = props.link?.favicon;
    fetching.value = false;
    void nextTick(() => urlInput.value?.focus());
  },
);

/** Prepend https:// when the user typed a bare domain; leave any URL that
 *  already carries a scheme alone (an unsupported scheme is rejected by the
 *  backend fetch/open rather than being mangled into a garbage https URL). */
function normalizeUrl(input: string): string {
  const trimmed = input.trim();
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  if (trimmed.includes("://")) return trimmed;
  return `https://${trimmed}`;
}

/** Auto-fetch title + favicon. Only fills the title when it is still empty, so
 *  a manual customization is never clobbered; the favicon is always refreshed. */
async function fetchMeta() {
  const target = normalizeUrl(url.value);
  if (!target || target === "https://" || fetching.value) return;
  fetching.value = true;
  try {
    const meta = await fetchUrlMeta(target);
    if (meta?.title && !title.value.trim()) title.value = meta.title;
    if (meta?.favicon) favicon.value = meta.favicon;
  } finally {
    fetching.value = false;
  }
}

function onSave() {
  const target = normalizeUrl(url.value);
  if (!target || target === "https://") return;
  emit("save", {
    title: title.value.trim() || target,
    url: target,
    favicon: favicon.value,
  });
}
</script>

<template>
  <BaseModal
    :open="props.open"
    :title="
      props.mode === 'edit'
        ? t('quickLinks.dialogEditTitle')
        : t('quickLinks.dialogAddTitle')
    "
    :subtitle="
      props.mode === 'edit'
        ? t('quickLinks.dialogEditSubtitle')
        : t('quickLinks.dialogAddSubtitle')
    "
    @close="emit('close')"
  >
    <div class="quick-link-form">
      <label class="field">
        <span>{{ t("quickLinks.dialogUrlLabel") }}</span>
        <div class="url-row">
          <input
            ref="urlInput"
            v-model="url"
            type="text"
            class="text-input"
            :placeholder="t('quickLinks.urlPlaceholder')"
            @keydown.enter.prevent="fetchMeta"
          />
          <button
            class="icon-button secondary fetch-btn"
            type="button"
            :disabled="fetching"
            :title="t('quickLinks.fetch')"
            :aria-label="t('quickLinks.fetch')"
            @click="fetchMeta"
          >
            <Loader2 v-if="fetching" class="spin" aria-hidden="true" />
            <RefreshCw v-else aria-hidden="true" />
          </button>
        </div>
      </label>

      <label class="field">
        <span>{{ t("quickLinks.dialogTitleLabel") }}</span>
        <input
          v-model="title"
          type="text"
          class="text-input"
          :placeholder="t('quickLinks.titlePlaceholder')"
          @keydown.enter.prevent="onSave"
        />
      </label>

      <p v-if="fetching" class="fetch-hint">{{ t("quickLinks.fetching") }}</p>
      <p v-else class="fetch-hint muted">{{ t("quickLinks.fetchHint") }}</p>
    </div>

    <template #footer>
      <button class="secondary" type="button" @click="emit('close')">
        {{ t("quickLinks.dialogCancel") }}
      </button>
      <button class="primary" type="button" @click="onSave">
        {{ t("quickLinks.dialogSave") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.quick-link-form {
  display: grid;
  gap: 14px;
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

.url-row {
  display: flex;
  gap: 8px;
}

.url-row .text-input {
  flex: 1;
  min-width: 0;
}

.fetch-btn {
  flex-shrink: 0;
  width: 34px;
  height: 34px;
}

.spin {
  animation: ql-spin 0.8s linear infinite;
}

@keyframes ql-spin {
  to {
    transform: rotate(360deg);
  }
}

.fetch-hint {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12px;
  line-height: 1.5;
}

.fetch-hint.muted {
  color: var(--text-muted);
}
</style>
