<script setup lang="ts">
import { markRaw, computed, nextTick, ref, watch, type Component } from "vue";
import {
  Activity,
  ArrowRightLeft,
  Download,
  ExternalLink,
  FileText,
  RefreshCw,
  Search,
  Settings,
  Sun,
  Trash2,
  Upload,
} from "@lucide/vue";
import { useI18n } from "vue-i18n";
import { useCommandPalette } from "../composables/useCommandPalette";
import { useVaultActions } from "../composables/useVaultActions";
import type { NavigationId } from "../composables/useNavigation";

const { t } = useI18n();
const { isOpen, close } = useCommandPalette();
const { navigate, runAction, toggleCurrentTheme, openStatus } = useVaultActions();

interface Command {
  id: string;
  label: string;
  group: "navigation" | "actions";
  icon: Component;
  run: () => void;
}

const navIcons = {
  documents: FileText,
  settings: Settings,
  trash: Trash2,
} as const;

const query = ref("");
const selectedId = ref<string | null>(null);
const inputEl = ref<HTMLInputElement | null>(null);

const commands = computed<Command[]>(() => {
  const navigation: Command[] = (
    ["documents", "trash", "settings"] as NavigationId[]
  ).map((id) => ({
    id: `nav-${id}`,
    label: t(`nav.${id}`),
    group: "navigation",
    icon: markRaw(navIcons[id]),
    run: () => navigate(id),
  }));
  // "状态" jumps straight to Settings > Status (unified tasks/archive view).
  navigation.push({
    id: "nav-status",
    label: t("settings.tabs.status"),
    group: "navigation",
    icon: markRaw(Activity),
    run: () => openStatus(),
  });

  const actions: Command[] = [
    {
      id: "act-open",
      label: t("actions.open"),
      group: "actions",
      icon: markRaw(ExternalLink),
      run: () => runAction("actionLogs.open"),
    },
    {
      id: "act-commit",
      label: t("actions.commit"),
      group: "actions",
      icon: markRaw(Upload),
      run: () => runAction("actionLogs.commit"),
    },
    {
      id: "act-export",
      label: t("actions.export"),
      group: "actions",
      icon: markRaw(Download),
      run: () => runAction("actionLogs.export"),
    },
    {
      id: "act-checkout",
      label: t("actions.checkout"),
      group: "actions",
      icon: markRaw(ArrowRightLeft),
      run: () => runAction("actionLogs.checkout"),
    },
    {
      id: "act-refresh",
      label: t("actions.refresh"),
      group: "actions",
      icon: markRaw(RefreshCw),
      run: () => runAction("actionLogs.refresh"),
    },
    {
      id: "act-theme",
      label: t("actions.toggleTheme"),
      group: "actions",
      icon: markRaw(Sun),
      run: () => toggleCurrentTheme(),
    },
  ];

  return [...navigation, ...actions];
});

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();

  if (!q) {
    return commands.value;
  }

  return commands.value.filter((command) =>
    command.label.toLowerCase().includes(q),
  );
});

const navigationFiltered = computed(() =>
  filtered.value.filter((command) => command.group === "navigation"),
);

const actionsFiltered = computed(() =>
  filtered.value.filter((command) => command.group === "actions"),
);

watch(filtered, (list) => {
  selectedId.value = list[0]?.id ?? null;
});

watch(isOpen, (open) => {
  if (open) {
    query.value = "";
    nextTick(() => inputEl.value?.focus());
  }
});

function moveSelection(delta: number) {
  const list = filtered.value;

  if (list.length === 0) {
    return;
  }

  const currentIndex = selectedId.value
    ? list.findIndex((command) => command.id === selectedId.value)
    : -1;
  let nextIndex = currentIndex + delta;

  if (nextIndex < 0) {
    nextIndex = list.length - 1;
  }
  if (nextIndex >= list.length) {
    nextIndex = 0;
  }

  selectedId.value = list[nextIndex].id;
}

function runSelected() {
  const command = filtered.value.find(
    (candidate) => candidate.id === selectedId.value,
  );

  if (command) {
    command.run();
    close();
  }
}

function runCommand(command: Command) {
  command.run();
  close();
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "ArrowDown") {
    event.preventDefault();
    moveSelection(1);
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    moveSelection(-1);
  } else if (event.key === "Enter") {
    event.preventDefault();
    runSelected();
  } else if (event.key === "Escape") {
    event.preventDefault();
    close();
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="isOpen"
      class="palette-overlay"
      @click="close"
      @keydown="onKeydown"
    >
      <div
        class="palette-panel"
        role="dialog"
        aria-modal="true"
        :aria-label="t('commandPalette.title')"
        @click.stop
      >
        <div class="palette-input-row">
          <Search aria-hidden="true" />
          <input
            ref="inputEl"
            v-model="query"
            type="text"
            :placeholder="t('commandPalette.placeholder')"
          />
        </div>

        <div class="palette-list">
          <p v-if="filtered.length === 0" class="palette-empty">
            {{ t("commandPalette.empty") }}
          </p>

          <template v-else>
            <div v-if="navigationFiltered.length" class="palette-group">
              <div class="palette-group-title">
                {{ t("commandPalette.groupNavigation") }}
              </div>
              <button
                v-for="command in navigationFiltered"
                :key="command.id"
                type="button"
                class="palette-item"
                :class="{ selected: selectedId === command.id }"
                @click="runCommand(command)"
                @mouseenter="selectedId = command.id"
              >
                <component :is="command.icon" aria-hidden="true" />
                <span>{{ command.label }}</span>
              </button>
            </div>

            <div v-if="actionsFiltered.length" class="palette-group">
              <div class="palette-group-title">
                {{ t("commandPalette.groupActions") }}
              </div>
              <button
                v-for="command in actionsFiltered"
                :key="command.id"
                type="button"
                class="palette-item"
                :class="{ selected: selectedId === command.id }"
                @click="runCommand(command)"
                @mouseenter="selectedId = command.id"
              >
                <component :is="command.icon" aria-hidden="true" />
                <span>{{ command.label }}</span>
              </button>
            </div>
          </template>
        </div>

        <footer class="palette-footer">{{ t("commandPalette.hint") }}</footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.palette-overlay {
  position: fixed;
  inset: 0;
  z-index: 50;
  display: grid;
  place-items: start center;
  padding-top: 12vh;
  background: rgb(15 23 36 / 45%);
  backdrop-filter: blur(3px);
}

.palette-panel {
  width: min(560px, 92vw);
  max-height: 70vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-surface);
  box-shadow: var(--overlay-shadow);
}

.palette-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-soft);
}

.palette-input-row svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--text-muted);
  fill: none;
  stroke: currentcolor;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 2;
}

.palette-input-row input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--text-primary);
}

.palette-list {
  min-height: 0;
  overflow: auto;
  padding: 8px;
}

.palette-empty {
  margin: 0;
  padding: 20px 12px;
  color: var(--text-muted);
  text-align: center;
}

.palette-group + .palette-group {
  margin-top: 4px;
}

.palette-group-title {
  padding: 6px 8px 4px;
  color: var(--text-secondary);
  font-size: 11px;
  font-weight: 700;
  text-transform: uppercase;
}

.palette-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: 0;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  text-align: left;
}

.palette-item svg {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  color: var(--text-muted);
  fill: none;
  stroke: currentcolor;
  stroke-linecap: round;
  stroke-linejoin: round;
  stroke-width: 2;
}

.palette-item.selected {
  background: var(--bg-active);
  color: var(--accent-text);
}

.palette-item.selected svg {
  color: var(--accent);
}

.palette-footer {
  padding: 8px 16px;
  border-top: 1px solid var(--border-soft);
  background: var(--bg-subtle);
  color: var(--text-muted);
  font-size: 12px;
}
</style>
