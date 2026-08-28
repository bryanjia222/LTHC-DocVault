<script setup lang="ts">
import { ref, watch, computed } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useVault, type VaultProbe } from "../composables/useVault";
import { useDesktopState } from "../composables/useDesktopState";

/*
 * Switch-backend / connect dialog. Drives the same `connect()` contract whether
 * reached from first-run onboarding or the Settings "Switch…" button: an empty
 * (or absent) directory creates a new vault there; a recognized vault directory
 * opens it. The directory field is pre-filled with the recommended location
 * (`~/.DocVault`) so a first-run user can create a vault with one click, or
 * browse to pick an existing one.
 *
 * The backend is only selectable for a directory that is not yet a vault: an
 * existing vault's backend is fixed by its config.toml, and connect_vault_core
 * opens it with that backend regardless of what is picked here. So once a
 * directory is probed as an existing vault, the selector is locked to the
 * vault's real backend with a hover tooltip explaining why - to use a different
 * backend the user must point at an empty directory. Errors map to localized
 * messages via ConnectError.
 */

const { t } = useI18n();
const { switchBackendOpen, closeSwitchBackend } = useDialogs();
const { connect, isTauri, recommendedRoot, probeVault } = useVault();
const desktop = useDesktopState();

const dir = ref("");
// New vaults default to restic - the local-copy backend is hidden from the
// selector (it is a dev/simple backend, not the recommended one). An existing
// vault whose backend is local-copy is still opened correctly: the dir watcher
// restores its real backend and the locked control shows it read-only.
const backend = ref<"local-copy" | "restic">("restic");
const password = ref("");
const confirmPassword = ref("");
const status = ref("");
const error = ref("");
const switching = ref(false);
/** Probe of the current `dir`: empty / existing / unrecognized. */
const probe = ref<VaultProbe>({ status: "empty" });
/** An existing vault's backend is fixed - lock the selector and show it. */
const backendLocked = computed(() => probe.value.status === "existing");

watch(switchBackendOpen, (open) => {
  if (!open) return;
  // Pre-fill the recommended location: submitting as-is creates a new vault
  // there (empty dir); browsing lets the user pick an existing vault to open.
  dir.value = recommendedRoot.value;
  // New vaults always default to restic (local-copy is hidden). An existing
  // vault's real backend is restored by the `dir` watcher once probed.
  backend.value = "restic";
  password.value = "";
  confirmPassword.value = "";
  status.value = "";
  error.value = "";
  switching.value = false;
  // `probe` refreshes via the `dir` watcher below; reset to a neutral state
  // until that async probe resolves so a stale lock from a previous open does
  // not flash.
  probe.value = { status: "empty" };
});

// Re-probe whenever the directory changes (pre-fill, browse, or manual edit) so
// the selector locks/unlocks to match what connect_vault_core will actually do.
// For an existing vault, drive the selector to the vault's real backend so the
// locked control displays the truth rather than the stale pre-fill.
watch(dir, async (d) => {
  if (!d) {
    probe.value = { status: "empty" };
    return;
  }
  try {
    probe.value = await probeVault(d);
  } catch {
    probe.value = { status: "empty" };
  }
  if (probe.value.status === "existing" && probe.value.backend) {
    backend.value = probe.value.backend === "restic" ? "restic" : "local-copy";
  }
});

async function pickDir() {
  if (!isTauri()) return;
  const result = await open({ directory: true, multiple: false });
  if (typeof result === "string") {
    dir.value = result;
  }
}

async function submit() {
  error.value = "";
  status.value = "";
  if (!dir.value) {
    error.value = t("connect.chooseDir");
    return;
  }
  // The password is typed twice so a typo can't silently lock a new vault.
  if (
    backend.value === "restic" &&
    !backendLocked.value &&
    password.value !== confirmPassword.value
  ) {
    error.value = t("connect.passwordMismatch");
    return;
  }
  switching.value = true;
  try {
    const outcome = await connect({
      root_dir: dir.value,
      backend: backend.value,
      restic_password:
        backend.value === "restic" && !backendLocked.value
          ? password.value
          : undefined,
    });
    // Desktop state (tags + tracked sources) is scoped per vault root, so
    // reload the slice for the now-active vault.
    await desktop.loadDesktopState();
    status.value =
      outcome.mode === "initialized"
        ? t("connect.initialized", { backend: t(`backend.${outcome.backend}`) })
        : t("connect.opened", { backend: t(`backend.${outcome.backend}`) });
    password.value = "";
    confirmPassword.value = "";
    // A successful connect/initialize is the signal itself - close the dialog
    // so the user lands back in the workspace with the new vault active.
    // (Errors keep the dialog open so the message stays visible.)
    close();
  } catch (e: unknown) {
    const err = e as { kind?: string; message?: string };
    if (err?.kind && err.kind !== "other") {
      error.value = t(`connect.${err.kind}`);
    } else {
      error.value = err?.message ?? String(e);
    }
  } finally {
    switching.value = false;
  }
}

function close() {
  closeSwitchBackend();
}
</script>

<template>
  <BaseModal
    :open="switchBackendOpen"
    :title="t('connect.title')"
    @close="close"
  >
    <form id="switch-backend-form" class="dialog-form" @submit.prevent="submit">
      <p class="dialog-hint">{{ t("connect.hint") }}</p>

      <label class="field">
        <span>{{ t("connect.dirLabel") }}</span>
        <div class="file-row">
          <input
            v-model="dir"
            type="text"
            class="text-input"
            :placeholder="t('connect.chooseDir')"
            readonly
          />
          <button type="button" @click="pickDir">
            {{ t("connect.browse") }}
          </button>
        </div>
      </label>

      <label class="field">
        <span>{{ t("connect.backend") }}</span>
        <!-- An existing vault's backend is fixed by its config.toml; show it
             read-only (this includes local-copy vaults - they are opened, just
             not offered as a choice for new vaults). -->
        <p v-if="backendLocked" class="backend-readonly">
          {{ t(`backend.${backend}`) }}
        </p>
        <!-- A new vault: only restic is offered (local-copy is hidden). -->
        <select v-else v-model="backend" class="text-input">
          <option value="restic">{{ t("backend.restic") }}</option>
        </select>
      </label>

      <p v-if="backendLocked" class="dialog-hint">
        {{ t("connect.backendLocked") }}
      </p>
      <p v-else class="dialog-hint">{{ t("connect.backendResticOnly") }}</p>

      <label v-if="backend === 'restic' && !backendLocked" class="field">
        <span>{{ t("connect.password") }}</span>
        <input
          v-model="password"
          type="password"
          class="text-input"
          :placeholder="t('connect.password')"
        />
      </label>
      <label v-if="backend === 'restic' && !backendLocked" class="field">
        <span>{{ t("connect.passwordConfirm") }}</span>
        <input
          v-model="confirmPassword"
          type="password"
          class="text-input"
          :placeholder="t('connect.passwordConfirm')"
        />
      </label>

      <p v-if="status" class="form-status">{{ status }}</p>
      <p v-if="error" class="form-error">{{ error }}</p>
    </form>

    <template #footer>
      <button class="secondary" type="button" @click="close">
        {{ t("actions.cancel") }}
      </button>
      <button
        class="primary"
        type="submit"
        form="switch-backend-form"
        :disabled="switching"
      >
        {{ t("connect.submit") }}
      </button>
    </template>
  </BaseModal>
</template>

<style scoped>
.dialog-form {
  display: grid;
  gap: 14px;
}

.dialog-hint {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.field {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.field span {
  color: var(--text-muted);
  font-size: 12px;
}

.text-input {
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
}

.text-input[readonly] {
  background: var(--bg-subtle);
}

select.text-input {
  padding-right: 28px;
}

.backend-readonly {
  margin: 0;
  height: 32px;
  display: flex;
  align-items: center;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-subtle);
  color: var(--text-primary);
  font-size: 13px;
}

.file-row {
  display: flex;
  gap: 8px;
}

.file-row .text-input {
  flex: 1;
  min-width: 0;
}

.file-row button {
  height: 32px;
  padding: 0 14px;
}

.form-status {
  margin: 0;
  color: var(--success-text);
  font-size: 13px;
}

.form-error {
  margin: 0;
  color: var(--danger-text);
  font-size: 13px;
}
</style>
