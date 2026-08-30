<script setup lang="ts">
import { FilePlus, Upload } from "@lucide/vue";
import { useI18n } from "vue-i18n";

import { useDocuments } from "../../composables/useDocuments";
import { useDialogs } from "../../composables/useDialogs";
import { useVaultActions } from "../../composables/useVaultActions";
import { TYPE_CATEGORIES } from "../../utils/typeCategory";
import type { SearchScope } from "../../utils/filter";

const { t } = useI18n();
const {
  filteredDocuments,
  searchQuery,
  searchScope,
  typeFilter,
  activeFilterCount,
  toggleType,
  clearFilters,
} = useDocuments();
const { openNewDocument } = useDialogs();
const { startImport } = useVaultActions();

/** The three user-facing type categories (文档 / PPT / 表格) for filter chips. */
const typeCategories = TYPE_CATEGORIES;

/** Search-scope dropdown change (cast the raw string to SearchScope). */
function onScopeChange(value: string) {
  searchScope.value = value as SearchScope;
}
</script>

<template>
  <div class="panel-header">
    <div>
      <h2>{{ t("documents.title") }}</h2>
      <p>
        {{ t("documents.visible", { count: filteredDocuments.length }) }}
      </p>
    </div>
    <div class="toolbar">
      <select
        class="search-scope"
        :value="searchScope"
        :aria-label="t('search.scopeLabel')"
        @change="onScopeChange(($event.target as HTMLSelectElement).value)"
      >
        <option value="all">{{ t("search.scope.all") }}</option>
        <option value="tags">{{ t("search.scope.tags") }}</option>
        <option value="filename">{{ t("search.scope.filename") }}</option>
        <option value="owner">{{ t("search.scope.owner") }}</option>
        <option value="id">{{ t("search.scope.id") }}</option>
      </select>
      <input
        v-model="searchQuery"
        type="search"
        :placeholder="t('documents.searchPlaceholder')"
        :aria-label="t('actions.search')"
      />
    </div>
  </div>

  <div class="filter-bar">
    <div class="filter-group">
      <span class="filter-label">{{ t("filters.type") }}</span>
      <button
        v-for="category in typeCategories"
        :key="category"
        type="button"
        class="chip"
        :class="{ active: typeFilter.has(category) }"
        @click="toggleType(category)"
      >
        {{ t(`filters.category.${category}`) }}
      </button>
    </div>

    <span class="filter-spacer"></span>

    <span v-if="activeFilterCount > 0" class="filter-count">{{
      t("filters.active", { count: activeFilterCount })
    }}</span>
    <button
      v-if="activeFilterCount > 0"
      type="button"
      class="chip clear"
      @click="clearFilters"
    >
      {{ t("filters.clear") }}
    </button>
    <button
      class="filter-action-btn"
      type="button"
      :title="t('actions.newDocument')"
      @click="openNewDocument()"
    >
      <FilePlus aria-hidden="true" />
      {{ t("actions.newDocument") }}
    </button>
    <button
      class="filter-action-btn"
      type="button"
      :title="t('actions.importDocument')"
      @click="startImport()"
    >
      <Upload aria-hidden="true" />
      {{ t("actions.importDocument") }}
    </button>
  </div>
</template>

<style scoped>
/* The panel-header base styles are global; keep the document title at the
   same size it had when DocumentsView owned the whole template. */
h2 {
  font-size: 18px;
}

input[type="search"] {
  width: 260px;
  height: 34px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  outline: none;
  background: var(--bg-surface);
  color: var(--text-primary);
}

input[type="search"]:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.search-scope {
  height: 34px;
  padding: 0 8px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 12px;
  cursor: pointer;
}

.search-scope:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
}

.filter-action-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--bg-surface);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
}

.filter-action-btn:hover:not(:disabled) {
  background: var(--bg-hover);
}

.filter-action-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.filter-action-btn svg {
  width: 15px;
  height: 15px;
  fill: none;
  stroke: currentcolor;
  stroke-width: 2;
}

.filter-bar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 8px;
  margin-bottom: 14px;
}

.filter-group {
  display: flex;
  align-items: center;
  gap: 6px;
}

.filter-label {
  color: var(--text-muted);
  font-size: 12px;
}

.filter-spacer {
  flex: 1;
}

.filter-count {
  color: var(--text-muted);
  font-size: 12px;
}

.chip {
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: 999px;
  background: var(--bg-surface);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.chip:hover {
  background: var(--bg-hover);
}

.chip.active {
  border-color: var(--accent);
  background: var(--accent-soft);
  color: var(--text-primary);
}

.chip.clear {
  color: var(--danger-text);
}
</style>
