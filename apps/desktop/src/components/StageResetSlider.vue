<script setup lang="ts">
import { ref, computed } from "vue";
import { useI18n } from "vue-i18n";
import { useVaultActions } from "../composables/useVaultActions";
import type { ResetStage, ResetBackend } from "../composables/useVault";

/*
 * Dev-only stage-reset slider. A draggable three-node track picks a reset
 * target; the confirm button hands it to `resetToStageAction`, which confirms
 * (destructive) and reloads state. Stages 2/3 expose a backend picker (plus a
 * restic password); stage 1 (fresh) wipes to onboarding, so no backend is
 * configured here - the user picks repo + backend on the onboarding screen.
 */

const { t } = useI18n();
const { resetToStageAction } = useVaultActions();

const STAGES: ResetStage[] = ["fresh", "initial", "seeded"];
const BACKENDS: ResetBackend[] = ["local-copy", "restic"];

const selectedIndex = ref(1); // default to "initial"
const backend = ref<ResetBackend>("local-copy");
const password = ref("");
const dragging = ref(false);
const trackEl = ref<HTMLElement | null>(null);

const stage = computed(() => STAGES[selectedIndex.value]);
const stageNumber = computed(() => selectedIndex.value + 1);
const stageLabel = computed(() =>
  t("dev.stageLabel", { n: stageNumber.value }),
);
const stageHint = computed(() => t(`dev.stages.${stage.value}.hint`));
const thumbPos = computed(
  () => (selectedIndex.value / (STAGES.length - 1)) * 100,
);
const needsBackend = computed(() => stage.value !== "fresh");
// The restic password is optional: the backend falls back to a dev default
// when blank, so confirm is always enabled (a stage is always selected).

function selectIndex(i: number) {
  selectedIndex.value = i;
}

function indexFromClientX(clientX: number): number {
  const el = trackEl.value;
  if (!el) return selectedIndex.value;
  const rect = el.getBoundingClientRect();
  const ratio = (clientX - rect.left) / rect.width;
  const clamped = Math.min(1, Math.max(0, ratio));
  return Math.round(clamped * (STAGES.length - 1));
}

function onPointerDown(e: PointerEvent) {
  dragging.value = true;
  (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  selectedIndex.value = indexFromClientX(e.clientX);
}
function onPointerMove(e: PointerEvent) {
  if (!dragging.value) return;
  selectedIndex.value = indexFromClientX(e.clientX);
}
function onPointerUp(e: PointerEvent) {
  if (!dragging.value) return;
  dragging.value = false;
  (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
    selectedIndex.value = Math.max(0, selectedIndex.value - 1);
    e.preventDefault();
  } else if (e.key === "ArrowRight" || e.key === "ArrowUp") {
    selectedIndex.value = Math.min(STAGES.length - 1, selectedIndex.value + 1);
    e.preventDefault();
  }
}

function onConfirm() {
  resetToStageAction(stage.value, backend.value, password.value || undefined);
}
</script>

<template>
  <div class="stage-reset">
    <p class="slider-hint">{{ t("dev.sliderHint") }}</p>

    <div
      ref="trackEl"
      class="stage-track"
      role="slider"
      :aria-valuemin="1"
      :aria-valuemax="STAGES.length"
      :aria-valuenow="stageNumber"
      :aria-valuetext="stageLabel"
      tabindex="0"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @keydown="onKeydown"
    >
      <div class="track-rail" />
      <div class="track-fill" :style="{ width: thumbPos + '%' }" />
      <button
        v-for="(s, i) in STAGES"
        :key="s"
        type="button"
        class="node"
        :class="{ active: i === selectedIndex }"
        :style="{ left: (i / (STAGES.length - 1)) * 100 + '%' }"
        :aria-label="t('dev.stageLabel', { n: i + 1 })"
        @click.stop="selectIndex(i)"
      />
      <div
        class="thumb"
        :class="{ dragging }"
        :style="{ left: thumbPos + '%' }"
      />
    </div>

    <div class="stage-labels">
      <div
        v-for="(s, i) in STAGES"
        :key="s"
        class="stage-label"
        :class="{ active: i === selectedIndex }"
      >
        <span class="stage-num">{{ t("dev.stageLabel", { n: i + 1 }) }}</span>
        <span class="stage-desc">{{ t(`dev.stages.${s}.name`) }}</span>
      </div>
    </div>

    <p class="stage-hint">{{ stageHint }}</p>

    <div v-if="needsBackend" class="stage-options">
      <label class="field">
        <span>{{ t("dev.backend") }}</span>
        <select v-model="backend" class="text-input">
          <option v-for="b in BACKENDS" :key="b" :value="b">
            {{ t(`backend.${b}`) }}
          </option>
        </select>
      </label>
      <label v-if="backend === 'restic'" class="field">
        <span>{{ t("dev.password") }}</span>
        <input
          v-model="password"
          type="password"
          class="text-input"
          :placeholder="t('dev.password')"
        />
        <span class="opt-hint">{{ t("dev.passwordOptional") }}</span>
      </label>
    </div>
    <p v-else class="fresh-note">{{ t("dev.freshNoBackend") }}</p>

    <div class="confirm-row">
      <button class="primary" type="button" @click="onConfirm">
        {{ t("dev.confirmStage", { stage: stageLabel }) }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.stage-reset {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 12px;
}

.slider-hint {
  margin: 0;
  color: var(--text-muted);
  font-size: 12px;
}

.opt-hint {
  color: var(--text-muted);
  font-size: 11px;
}

.stage-track {
  position: relative;
  height: 28px;
  display: flex;
  align-items: center;
  cursor: pointer;
  touch-action: none;
  outline: none;
}

.stage-track:focus-visible .thumb {
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.track-rail,
.track-fill {
  position: absolute;
  left: 0;
  top: 50%;
  transform: translateY(-50%);
  height: 6px;
  border-radius: 3px;
}

.track-rail {
  right: 0;
  background: var(--bg-inset);
}

.track-fill {
  background: var(--accent);
}

.node {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 14px;
  height: 14px;
  padding: 0;
  border: 2px solid var(--border-strong);
  border-radius: 50%;
  background: var(--bg-surface);
  cursor: pointer;
}

.node.active {
  border-color: var(--accent);
  background: var(--accent-soft);
}

.thumb {
  position: absolute;
  top: 50%;
  transform: translate(-50%, -50%);
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--accent);
  border: 2px solid var(--bg-surface);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
  pointer-events: none;
  transition: left 0.12s ease;
}

.thumb.dragging {
  transition: none;
}

.stage-labels {
  display: flex;
  margin-top: 2px;
}

.stage-label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1 1 0;
  text-align: center;
  align-items: center;
}

.stage-label:first-child {
  align-items: flex-start;
  text-align: left;
}

.stage-label:last-child {
  align-items: flex-end;
  text-align: right;
}

.stage-num {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.stage-label.active .stage-num {
  color: var(--accent);
}

.stage-desc {
  font-size: 11px;
  color: var(--text-muted);
}

.stage-hint {
  margin: 4px 0 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-inset);
  color: var(--text-secondary);
  font-size: 13px;
}

.stage-options {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}

.field {
  display: grid;
  gap: 4px;
  flex: 1 1 180px;
  min-width: 0;
}

.field span {
  color: var(--text-muted);
  font-size: 12px;
}

.fresh-note {
  margin: 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  background: var(--accent-bg);
  color: var(--accent-text);
  font-size: 13px;
}

.confirm-row {
  display: flex;
  justify-content: flex-end;
  margin-top: 4px;
}

.confirm-row .primary {
  height: 34px;
  padding: 0 18px;
}
</style>
