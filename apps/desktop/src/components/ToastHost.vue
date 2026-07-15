<script setup lang="ts">
import { useI18n } from "vue-i18n";
import { Loader2, CheckCircle2, XCircle, AlertCircle, X } from "@lucide/vue";
import { useToasts, type Toast } from "../composables/useToasts";

/*
 * Bottom-right task toasts. Mirrors the authoritative job state (from
 * useVault.subscribeJobs -> useToasts.onJobUpdate) into a transient bubble so
 * the user sees slow operations are running without opening the job center.
 * Each toast auto-dismisses a few seconds after its job goes terminal; the
 * close button dismisses early.
 */

const { t } = useI18n();
const { toasts, dismiss } = useToasts();

function actionLabel(toast: Toast): string {
  return t(`jobs.${toast.kind}`);
}

function statusText(toast: Toast): string {
  switch (toast.status) {
    case "running":
      return t("toast.running");
    case "succeeded":
      return t("toast.succeeded");
    case "failed":
      return t("toast.failed");
    default:
      return t("toast.cancelled");
  }
}
</script>

<template>
  <div class="toast-host" role="status" aria-live="polite">
    <div
      v-for="toast in toasts"
      :key="toast.id"
      class="toast"
      :data-status="toast.status"
    >
      <div class="toast-icon">
        <Loader2 v-if="toast.status === 'running'" class="spin" :size="16" />
        <CheckCircle2 v-else-if="toast.status === 'succeeded'" :size="16" />
        <XCircle v-else-if="toast.status === 'failed'" :size="16" />
        <AlertCircle v-else :size="16" />
      </div>
      <div class="toast-body">
        <strong>{{ actionLabel(toast) }} · {{ toast.label }}</strong>
        <span class="toast-status">{{ statusText(toast) }}</span>
        <span v-if="toast.error" class="toast-error" :title="toast.error">
          {{ toast.error }}
        </span>
      </div>
      <button
        class="toast-close"
        type="button"
        :aria-label="t('toast.dismiss')"
        @click="dismiss(toast.id)"
      >
        <X :size="14" />
      </button>
    </div>
  </div>
</template>

<style scoped>
.toast-host {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 10px;
  max-width: 360px;
  pointer-events: none;
}

.toast {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 12px 14px;
  background: var(--bg-surface);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.28);
  pointer-events: auto;
}

.toast-icon {
  display: flex;
  align-items: center;
  padding-top: 1px;
  color: var(--text-muted);
  flex-shrink: 0;
}

.toast[data-status="running"] .toast-icon {
  color: var(--text-secondary);
}
.toast[data-status="succeeded"] .toast-icon {
  color: var(--success-text);
}
.toast[data-status="failed"] .toast-icon,
.toast[data-status="cancelled"] .toast-icon {
  color: var(--danger-text);
}

.toast-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
  flex: 1;
}

.toast-body strong {
  font-size: 13px;
  color: var(--text-primary);
  font-weight: 600;
  word-break: break-all;
}

.toast-status {
  font-size: 12px;
  color: var(--text-muted);
}

.toast-error {
  font-size: 12px;
  color: var(--danger-text);
  word-break: break-all;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.toast-close {
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
  border: 0;
  color: var(--text-muted);
  cursor: pointer;
  padding: 2px;
  flex-shrink: 0;
}

.toast-close:hover {
  color: var(--text-primary);
}

.spin {
  animation: toast-spin 0.9s linear infinite;
}

@keyframes toast-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
