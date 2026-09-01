<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { X } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useCompare } from "../composables/useCompare";
import { getDocxodus } from "../composables/useDocxodus";
import { useVault } from "../composables/useVault";
import {
  captureHtml,
  compareCacheKey,
  getPreviewCache,
  setPreviewCache,
} from "../utils/previewCache";
import { reportBackendCommandError, reportError } from "../utils/reportError";

/*
 * Full-screen redline comparison overlay. Fetches both (document, version)
 * pairs named by the compare target, runs the Docxodus redline diff in its
 * worker, and renders the tracked-changes HTML. Results are cached with the
 * same memory + disk tiers as document previews.
 */

const { t } = useI18n();
const { compareTarget, closeCompare } = useCompare();
const { previewVersion, readPreviewCache, writePreviewCache } = useVault();

const loading = ref(false);
const error = ref<string | null>(null);
const container = ref<HTMLDivElement | null>(null);

// Each target change mints a guard; superseded loads stop before painting.
let activeGuard = 0;

function currentCompareCacheKey(): string {
  const target = compareTarget.value;
  if (!target) return "compare|none";
  const { old, new: next } = target;
  return compareCacheKey(
    old.document.id,
    old.version.label,
    next.document.id,
    next.version.label,
  );
}

async function fetchVersionBytes(
  documentId: string,
  versionLabel: string,
  scope: string,
): Promise<Uint8Array | null> {
  try {
    const bytes = await previewVersion({
      document_id: documentId,
      version: versionLabel,
    });
    return bytes ? new Uint8Array(bytes) : null;
  } catch (caught) {
    reportBackendCommandError(scope, caught);
    throw caught;
  }
}

async function load() {
  const target = compareTarget.value;
  if (!target) return;
  const guard = ++activeGuard;

  loading.value = true;
  error.value = null;
  if (container.value) container.value.innerHTML = "";

  const key = currentCompareCacheKey();
  const memoryHit = getPreviewCache(key);
  if (memoryHit) {
    if (container.value) container.value.innerHTML = memoryHit;
    loading.value = false;
    return;
  }

  let diskHit: string | null = null;
  try {
    diskHit = await readPreviewCache(key);
  } catch (caught) {
    reportBackendCommandError("compare.cache-read", caught);
  }
  if (guard !== activeGuard) return;
  if (diskHit) {
    setPreviewCache(key, diskHit);
    if (container.value) container.value.innerHTML = diskHit;
    loading.value = false;
    return;
  }

  try {
    const oldBytes = await fetchVersionBytes(
      target.old.document.id,
      target.old.version.label,
      "compare.fetch-old",
    );
    if (guard !== activeGuard) return;
    const newBytes = await fetchVersionBytes(
      target.new.document.id,
      target.new.version.label,
      "compare.fetch-new",
    );
    if (guard !== activeGuard) return;
    if (!oldBytes || !newBytes) {
      error.value = t("compare.noBytes");
      return;
    }
    const engine = await getDocxodus();
    if (guard !== activeGuard) return;
    const html = await engine.compareDocumentsToHtml(oldBytes, newBytes, {
      authorName: t("compare.authorName"),
      renderTrackedChanges: true,
    });
    if (guard !== activeGuard) return;
    const article = document.createElement("div");
    article.className = "preview-docx";
    article.innerHTML = html;
    if (container.value) container.value.appendChild(article);
    void cacheResult(container.value, key);
  } catch (caught) {
    if (guard === activeGuard) {
      error.value = String(caught instanceof Error ? caught.message : caught);
      reportError("compare.diff", caught);
    }
  } finally {
    if (guard === activeGuard) loading.value = false;
  }
}

async function cacheResult(el: HTMLElement | null, key: string) {
  if (!el) return;
  try {
    const html = await captureHtml(el);
    setPreviewCache(key, html);
    void writePreviewCache(key, html).catch((caught) =>
      reportBackendCommandError("compare.cache-write", caught),
    );
  } catch (caught) {
    reportError("compare.capture", caught);
  }
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    closeCompare();
  }
}

watch(compareTarget, () => {
  void load();
});

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  void load();
});

onBeforeUnmount(() => {
  activeGuard++;
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <Teleport to="body">
    <div class="preview-overlay" @click.self="closeCompare">
      <div
        class="preview-modal surface"
        role="dialog"
        aria-modal="true"
        :aria-label="t('compare.title')"
        @click.stop
      >
        <header class="preview-header">
          <div class="preview-heading">
            <h2>{{ t("compare.title") }}</h2>
            <p v-if="compareTarget">
              {{
                t("compare.resultSubtitle", {
                  old: compareTarget.old.document.name,
                  oldVersion: compareTarget.old.version.label,
                  new: compareTarget.new.document.name,
                  newVersion: compareTarget.new.version.label,
                })
              }}
            </p>
          </div>
          <button
            class="icon-button secondary"
            type="button"
            :aria-label="t('compare.close')"
            :title="t('compare.close')"
            @click="closeCompare"
          >
            <X aria-hidden="true" />
          </button>
        </header>

        <div class="preview-body">
          <div v-if="loading" class="preview-status">
            {{ t("compare.loading") }}
          </div>
          <div v-else-if="error" class="preview-status preview-error">
            {{ t("compare.error", { error }) }}
          </div>
          <div ref="container" class="preview-content" />
        </div>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
/* Overlay + modal geometry mirror DocumentPreview so both full-screen
   surfaces look identical; only the header subtitle and content differ. */
.preview-overlay {
  position: fixed;
  inset: 0;
  z-index: 70;
  display: grid;
  place-items: center;
  padding: 4vh 16px;
  background: rgb(15 23 36 / 55%);
  backdrop-filter: blur(3px);
}

.preview-modal {
  position: relative;
  width: min(1100px, 96vw);
  height: 92vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.preview-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-soft);
}

.preview-heading h2 {
  font-size: 16px;
  font-weight: 700;
}

.preview-heading p {
  margin-top: 2px;
  color: var(--text-muted);
  font-size: 12px;
}

.preview-header .icon-button {
  flex-shrink: 0;
}

.preview-body {
  position: relative;
  min-height: 0;
  flex: 1;
  overflow-y: scroll;
  overflow-x: hidden;
  padding: 18px;
}

.preview-status {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  gap: 8px;
  text-align: center;
  color: var(--text-muted);
  font-size: 14px;
  background: var(--bg-surface);
}

.preview-error {
  color: var(--danger-text);
}

.preview-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
}
</style>
