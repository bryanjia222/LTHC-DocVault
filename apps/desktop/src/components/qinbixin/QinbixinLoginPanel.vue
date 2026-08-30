<script setup lang="ts">
import { ref } from "vue";
import { Loader2 } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useQinbixin } from "../../composables/useQinbixin";

const { t } = useI18n();
const { login } = useQinbixin();

const userName = ref("");
const password = ref("");
const loggingIn = ref(false);

async function submitLogin() {
  if (!userName.value.trim() || !password.value || loggingIn.value) return;
  loggingIn.value = true;
  const ok = await login(userName.value.trim(), password.value);
  loggingIn.value = false;
  if (ok) password.value = "";
}
</script>

<template>
  <form class="login-form" @submit.prevent="submitLogin">
    <label class="field">
      <span>{{ t("qinbixin.userName") }}</span>
      <input
        v-model="userName"
        class="text-input"
        type="text"
        autocomplete="username"
        :placeholder="t('qinbixin.userNamePlaceholder')"
      />
    </label>
    <label class="field">
      <span>{{ t("qinbixin.password") }}</span>
      <input
        v-model="password"
        class="text-input"
        type="password"
        autocomplete="current-password"
      />
    </label>
    <button class="primary login-button" type="submit" :disabled="loggingIn">
      <Loader2 v-if="loggingIn" class="spin" aria-hidden="true" />
      {{ t("qinbixin.login") }}
    </button>
  </form>
</template>

<style scoped>
.login-form {
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
  min-width: 0;
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

.login-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  height: 34px;
  padding: 0 16px;
  justify-self: start;
}

.spin {
  width: 16px;
  height: 16px;
  animation: qinbixin-login-spin 0.8s linear infinite;
}

@keyframes qinbixin-login-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
