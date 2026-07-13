<script setup lang="ts">
import { ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "vue-i18n";
import BaseModal from "./BaseModal.vue";
import { useDialogs } from "../composables/useDialogs";
import { useVault } from "../composables/useVault";

/*
 * Switch-backend dialog. Moves the connect form out of the Settings page into a
 * modal triggered by a button. Same fields + connect() contract as before; the
 * backend select pre-fills to the current backend so switching is a small edit,
 * not a from-scratch choice. Errors map to localized messages via ConnectError.
 */

const { t } = useI18n();
const { switchBackendOpen, closeSwitchBackend } = useDialogs();
const { config, connect, isTauri } = useVault();

const dir = ref("");
const backend = ref<"local-copy" | "restic">("local-copy");
const password = ref("");
const status = ref("");
const error = ref("");
const switching = ref(false);

watch(switchBackendOpen, (open) => {
  if (!open) return;
  dir.value = "";
  backend.value = config.value.backend === "restic" ? "restic" : "local-copy";
  password.value = "";
  status.value = "";
  error.value = "";
  switching.value = false;
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
  switching.value = true;
  try {
    const outcome = await connect({
      root_dir: dir.value,
      backend: backend.value,
      restic_password:
        backend.value === "restic" ? password.value : undefined,
    });
    status.value =
      outcome.mode === "initialized"
        ? t("connect.initialized", { backend: t(`backend.${outcome.backend}`) })
        : t("connect.opened", { backend: t(`backend.${outcome.backend}`) });
    password.value = "";
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
  <BaseModal :open="switchBackendOpen" :title="t('connect.title')" @close="close">
    <form id="switch-backend-form" class="dialog-form" @submit.prevent="submit">
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
        <select v-model="backend" class="text-input">
          <option value="local-copy">{{ t("backend.local-copy") }}</option>
          <option value="restic">{{ t("backend.restic") }}</option>
        </select>
      </label>

      <label v-if="backend === 'restic'" class="field">
        <span>{{ t("connect.password") }}</span>
        <input
          v-model="password"
          type="password"
          class="text-input"
          :placeholder="t('connect.password')"
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
