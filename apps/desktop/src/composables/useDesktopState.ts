import { invoke } from "@tauri-apps/api/core";
import { computed, ref, type ComputedRef, type Ref } from "vue";

import {
  desktopState as mockDesktopState,
  mockProbes,
  type DesktopState,
  type FileProbe,
  type FileStat,
  type ModificationStatus,
  type ProjectDef,
  type SortPref,
  type TrackedFile,
  type TrashedVersion,
} from "../data/mock";
import {
  mapDesktopState,
  mapFileProbe,
  mapFileStat,
  type RawDesktopState,
  type RawFileProbe,
  type RawFileStat,
  type RawProjectDef,
  type RawTrackedFile,
  type RawTrashedVersion,
} from "../utils/mappers";
import {
  MODIFICATION_HASH_THRESHOLD_BYTES,
  deriveModificationStatus,
} from "../utils/tracking";
import { isTauri } from "./useVault";

/*
 * Desktop-local annotations: tags + tracked source files. Backed by the
 * get_desktop_state / set_desktop_state / stat_files / probe_file Tauri commands
 * (per-vault-root scoped JSON in desktop-state.json). In pure browser dev it
 * falls back to the mock fixtures so the UI still renders. The module-level
 * reactive refs are app-wide singletons, like useVault's.
 *
 * This composable deliberately does NOT import useVault's document list: the
 * pending-track resolution that needs a post-commit document refresh is
 * orchestrated by App.vue (which already has useVault), keeping the two modules
 * decoupled.
 */

// --- shared reactive state (module-level singletons) ---

const tags: Ref<Record<string, string[]>> = ref({});
const tracked: Ref<TrackedFile[]> = ref([]);
/** User-created project folders for grouping documents (desktop-local, like tags). */
const projects: Ref<ProjectDef[]> = ref([]);
/** documentId -> its single projectId (absent key = unassigned; one-to-many). */
const assignments: Ref<Record<string, string>> = ref({});
/** Per-view persisted table sort: scope key (project id or "__all__") -> sort pref. */
const sortPrefs: Ref<Record<string, SortPref>> = ref({});
/** Document ids soft-deleted to the recycle bin (desktop-local hide). The vault
 *  still holds these docs + history until permanently deleted from the bin. */
const trashed: Ref<string[]> = ref([]);
/** Versions soft-deleted to the recycle bin, scoped by their document (version
 *  ids are unique only within a document, so the pair identifies one entry).
 *  Like `trashed`, this is a desktop-local hide; the vault holds the version
 *  until it is permanently deleted from the bin. */
const trashedVersions: Ref<TrashedVersion[]> = ref([]);
/** Latest probe per document id, populated by refreshModifications. */
const probes: Ref<Record<string, FileProbe>> = ref({});
const loaded: Ref<boolean> = ref(false);

/** A pending source-file tracking request, keyed by the commit job id that
 * produced it. Resolved by App.vue when the commit job succeeds. The "new"
 * variant carries no path: under the library model the tracked path is the
 * tool-owned library copy, derived from the doc id at resolution time. */
export type PendingTrack =
  | { kind: "known"; docId: string; path: string }
  | { kind: "new"; name: string; snapshotIds: string[] };

const pendingTracks: Ref<Record<string, PendingTrack>> = ref({});

let refreshInFlight = false;

const allTags: ComputedRef<string[]> = computed(() => {
  const set = new Set<string>();
  for (const list of Object.values(tags.value)) {
    for (const tag of list) set.add(tag);
  }
  return [...set].sort();
});

// --- load / save ---

async function loadDesktopState(): Promise<void> {
  if (!isTauri()) {
    tags.value = structuredClone(mockDesktopState.tags);
    tracked.value = mockDesktopState.tracked.map((t) => ({ ...t }));
    projects.value = mockDesktopState.projects.map((p) => ({ ...p }));
    assignments.value = { ...mockDesktopState.assignments };
    sortPrefs.value = { ...mockDesktopState.sortPrefs };
    trashed.value = [...mockDesktopState.trashed];
    trashedVersions.value = mockDesktopState.trashedVersions.map((v) => ({ ...v }));
    loaded.value = true;
    return;
  }
  try {
    const raw = await invoke<RawDesktopState>("get_desktop_state");
    const state: DesktopState = mapDesktopState(raw);
    tags.value = state.tags;
    tracked.value = state.tracked;
    projects.value = state.projects;
    assignments.value = state.assignments;
    sortPrefs.value = state.sortPrefs;
    trashed.value = state.trashed;
    trashedVersions.value = state.trashedVersions;
  } catch (e) {
    console.error("loadDesktopState failed", e);
  } finally {
    loaded.value = true;
  }
}

/** View-model TrackedFile -> the snake_case payload set_desktop_state expects. */
function toRawTrackedFile(file: TrackedFile): RawTrackedFile {
  const raw: RawTrackedFile = {
    document_id: file.documentId,
    path: file.path,
    size: file.size,
    mtime_ms: file.mtimeMs,
  };
  // Omit sha256 when null so serde deserializes it as None (large files).
  if (file.sha256) raw.sha256 = file.sha256;
  return raw;
}

/** View-model ProjectDef -> the snake_case payload (parent_id) the Rust
 *  `ProjectDef` deserializes. `parent_id` is omitted when null so root projects
 *  serialize cleanly (serde defaults it to None on the other side). */
function toRawProject(project: ProjectDef): RawProjectDef {
  const raw: RawProjectDef = { id: project.id, name: project.name };
  if (project.parentId !== null) raw.parent_id = project.parentId;
  return raw;
}

/** View-model TrashedVersion -> the snake_case payload the Rust
 *  `TrashedVersion` deserializes. */
function toRawTrashedVersion(version: TrashedVersion): RawTrashedVersion {
  return { document_id: version.documentId, version_id: version.versionId };
}

async function saveDesktopState(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("set_desktop_state", {
      tags: tags.value,
      tracked: tracked.value.map(toRawTrackedFile),
      projects: projects.value.map(toRawProject),
      assignments: assignments.value,
      sort_prefs: sortPrefs.value,
      trashed: trashed.value,
      trashed_versions: trashedVersions.value.map(toRawTrashedVersion),
    });
  } catch (e) {
    console.error("saveDesktopState failed", e);
  }
}

// --- tags ---

function setDocumentTags(docId: string, newTags: string[]): void {
  const next = { ...tags.value };
  if (newTags.length === 0) delete next[docId];
  else next[docId] = [...newTags];
  tags.value = next;
  void saveDesktopState();
}

function addTag(docId: string, tag: string): void {
  const clean = tag.trim();
  if (!clean) return;
  const current = tags.value[docId] ?? [];
  if (current.includes(clean)) return;
  setDocumentTags(docId, [...current, clean]);
}

function removeTag(docId: string, tag: string): void {
  const current = tags.value[docId] ?? [];
  setDocumentTags(
    docId,
    current.filter((t) => t !== tag),
  );
}

// --- projects (desktop-local folders for grouping documents) ---

/** Stable id for a new project. Uses crypto.randomUUID when available (secure
 * webview / Node 16+), falling back to a random string for older runtimes. */
function makeProjectId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }
  return `proj-${Math.random().toString(36).slice(2)}${Math.random().toString(36).slice(2)}`;
}

/**
 * Create a project folder under `parentId` (null for a root project) and
 * persist it, returning the new id. Rejects empty/whitespace names and names
 * already in use among the project's *siblings* (same parent, case-insensitive)
 * by returning null, so the caller can surface a validation error. Sibling-level
 * uniqueness (not global) lets two sub-projects under different parents share a
 * name.
 */
function createProject(parentId: string | null, name: string): string | null {
  const clean = name.trim();
  if (!clean) return null;
  const parent = parentId ?? null;
  const lower = clean.toLowerCase();
  const isSibling = (p: ProjectDef) => (p.parentId ?? null) === parent;
  if (projects.value.some((p) => isSibling(p) && p.name.toLowerCase() === lower)) {
    return null;
  }
  const id = makeProjectId();
  projects.value = [...projects.value, { id, name: clean, parentId: parent }];
  void saveDesktopState();
  return id;
}

/** Rename a project. Returns false (no-op) on empty name, a name taken by a
 * sibling (same parent), or an unknown id. */
function renameProject(id: string, name: string): boolean {
  const clean = name.trim();
  if (!clean) return false;
  const target = projects.value.find((p) => p.id === id);
  if (!target) return false;
  const parent = target.parentId ?? null;
  const lower = clean.toLowerCase();
  const isSibling = (p: ProjectDef) =>
    p.id !== id && (p.parentId ?? null) === parent;
  if (projects.value.some((p) => isSibling(p) && p.name.toLowerCase() === lower)) {
    return false;
  }
  projects.value = projects.value.map((p) => (p.id === id ? { ...p, name: clean } : p));
  void saveDesktopState();
  return true;
}

/**
 * Delete a project folder. Its direct children are re-parented to the deleted
 * project's parent (so a deleted sub-project's sub-projects move up rather than
 * becoming orphaned). Documents assigned to it are not deleted - the project id
 * is simply dropped from each document's membership list, so the doc remains in
 * the vault (and any other projects it belongs to).
 */
function deleteProject(id: string): void {
  const target = projects.value.find((p) => p.id === id);
  if (!target) return;
  const newParent = target.parentId ?? null;
  projects.value = projects.value.map((p) =>
    p.id === id
      ? p // removed below
      : (p.parentId ?? null) === id
        ? { ...p, parentId: newParent }
        : p,
  );
  projects.value = projects.value.filter((p) => p.id !== id);
  const next: Record<string, string> = {};
  for (const [docId, pid] of Object.entries(assignments.value)) {
    if (pid !== id) next[docId] = pid;
  }
  assignments.value = next;
  // Drop the deleted project's persisted sort pref (no longer reachable).
  if (sortPrefs.value[id]) {
    const nextPrefs = { ...sortPrefs.value };
    delete nextPrefs[id];
    sortPrefs.value = nextPrefs;
  }
  void saveDesktopState();
}

/** The single project a document belongs to, or null when unassigned. */
function projectOf(docId: string): string | null {
  return assignments.value[docId] ?? null;
}

/** Set a document's single project (replacing any previous assignment).
 *  Idempotent when the doc is already in this project. Persisted. Unknown
 *  project ids are ignored. */
function setDocumentProject(docId: string, projectId: string): void {
  if (!projects.value.some((p) => p.id === projectId)) return;
  if (assignments.value[docId] === projectId) return;
  assignments.value = { ...assignments.value, [docId]: projectId };
  void saveDesktopState();
}

/** Clear a document's project assignment (it becomes unassigned). Persisted;
 *  no-op when the doc is not assigned to any project. */
function clearDocumentProject(docId: string): void {
  if (!(docId in assignments.value)) return;
  const nextAssign = { ...assignments.value };
  delete nextAssign[docId];
  assignments.value = nextAssign;
  void saveDesktopState();
}

/** True when `ancestorId` is `id` itself or an ancestor of `id` (walking the
 *  parent chain). Used to forbid reparenting a project under its own
 *  descendant, which would create a cycle, and to test project-subtree
 *  membership when scoping/grouping the document list. */
function isAncestorOrSelf(id: string, ancestorId: string): boolean {
  let cursor: string | null = id;
  // Bound the walk by the project count so a malformed cycle can't loop forever.
  for (let i = 0; i <= projects.value.length && cursor; i++) {
    if (cursor === ancestorId) return true;
    const node = projects.value.find((p) => p.id === cursor);
    cursor = node?.parentId ?? null;
  }
  return false;
}

/** The full display path of a project from the root, names joined by " / "
 *  (e.g. "Work / ProjectA / Sub"). Used as the group-header label when the
 *  documents table is grouped by project. Walks the parent chain upward
 *  (bounded like `isAncestorOrSelf`); a root project yields just its name. */
function projectPath(id: string): string {
  const names: string[] = [];
  let cursor: string | null = id;
  for (let i = 0; i <= projects.value.length && cursor; i++) {
    const node = projects.value.find((p) => p.id === cursor);
    if (!node) break;
    names.unshift(node.name);
    cursor = node.parentId;
  }
  return names.join(" / ");
}

/**
 * Move a project under a new parent (`newParentId` null = root). Refuses to
 * reparent a project under itself or one of its own descendants (would create a
 * cycle) and returns false; unknown ids return false. Persisted.
 */
function reparentProject(id: string, newParentId: string | null): boolean {
  if (!projects.value.some((p) => p.id === id)) return false;
  const parent = newParentId ?? null;
  if (parent !== null) {
    if (parent === id) return false;
    if (!projects.value.some((p) => p.id === parent)) return false;
    // A cycle would form if `id` is already an ancestor-or-self of the new
    // parent (moving `id` under its own descendant).
    if (isAncestorOrSelf(parent, id)) return false;
  }
  projects.value = projects.value.map((p) =>
    p.id === id ? { ...p, parentId: parent } : p,
  );
  void saveDesktopState();
  return true;
}

/** The persisted table sort for a view (project id or "__all__"), or null. */
function getSortPref(scope: string): SortPref | null {
  return sortPrefs.value[scope] ?? null;
}

/** Persist the table sort for a view. `key`/`direction` are stored verbatim
 *  (strings) and re-validated by the composable when read back. */
function setSortPref(scope: string, key: string, direction: string): void {
  sortPrefs.value = { ...sortPrefs.value, [scope]: { key, direction } };
  void saveDesktopState();
}

// --- recycle bin (desktop-local soft-delete) ---

/** True when the document is soft-deleted into the recycle bin (hidden). */
function isTrashed(docId: string): boolean {
  return trashed.value.includes(docId);
}

/**
 * Move a document to the recycle bin: a desktop-local hide. The vault still
 * holds the document and all its history; the user can restore it or
 * permanently delete it from the bin. No-op when already trashed. Persisted.
 */
function trashDoc(docId: string): void {
  if (trashed.value.includes(docId)) return;
  trashed.value = [...trashed.value, docId];
  void saveDesktopState();
}

/**
 * Restore a document from the recycle bin (un-hide). The document's tags /
 * tracked source are untouched while trashed, so it reappears exactly as it was.
 * No-op when not trashed. Persisted.
 */
function restoreDoc(docId: string): void {
  if (!trashed.value.includes(docId)) return;
  trashed.value = trashed.value.filter((id) => id !== docId);
  void saveDesktopState();
}

/** All document ids currently in the recycle bin. */
function trashedIds(): string[] {
  return trashed.value;
}

// --- version recycle bin (desktop-local soft-delete per version) ---

/** True when the version is soft-deleted into the recycle bin (hidden). */
function isVersionTrashed(docId: string, versionId: string): boolean {
  return trashedVersions.value.some(
    (v) => v.documentId === docId && v.versionId === versionId,
  );
}

/**
 * Move a version to the recycle bin: a desktop-local hide. The vault still
 * holds the version until it is permanently deleted from the bin. No-op when
 * already trashed. Persisted.
 */
function trashVersion(docId: string, versionId: string): void {
  if (isVersionTrashed(docId, versionId)) return;
  trashedVersions.value = [
    ...trashedVersions.value,
    { documentId: docId, versionId },
  ];
  void saveDesktopState();
}

/**
 * Restore a version from the recycle bin (un-hide). No-op when not trashed.
 * Persisted.
 */
function restoreVersion(docId: string, versionId: string): void {
  if (!isVersionTrashed(docId, versionId)) return;
  trashedVersions.value = trashedVersions.value.filter(
    (v) => !(v.documentId === docId && v.versionId === versionId),
  );
  void saveDesktopState();
}

/** All trashed-version entries currently in the recycle bin. */
function trashedVersionList(): TrashedVersion[] {
  return trashedVersions.value;
}

/**
 * Remove a single version's recycle-bin membership (desktop-local hide only).
 * Called after the version is permanently deleted from the vault so no orphaned
 * entry lingers in desktop-state.json. The version's archive is not touched.
 */
function clearVersion(docId: string, versionId: string): void {
  if (!isVersionTrashed(docId, versionId)) return;
  trashedVersions.value = trashedVersions.value.filter(
    (v) => !(v.documentId === docId && v.versionId === versionId),
  );
  void saveDesktopState();
}

// --- tracked source files ---

function trackedFor(docId: string): TrackedFile | undefined {
  return tracked.value.find((t) => t.documentId === docId);
}

function trackedPathFor(docId: string): string | null {
  return trackedFor(docId)?.path ?? null;
}

function modificationFor(docId: string): ModificationStatus {
  return deriveModificationStatus(trackedFor(docId), probes.value[docId] ?? null);
}

/**
 * Record (or replace) a document's tracked source file and immediately sync the
 * probe cache to the new baseline, so the status reads "unchanged" right away
 * (the file was just imported / re-specified). Persists the change.
 */
function setTracked(file: TrackedFile): void {
  const next = tracked.value.filter((t) => t.documentId !== file.documentId);
  next.push(file);
  tracked.value = next;
  probes.value = {
    ...probes.value,
    [file.documentId]: {
      exists: true,
      size: file.size,
      mtimeMs: file.mtimeMs,
      sha256: file.sha256 ?? null,
    },
  };
  void saveDesktopState();
}

function clearTracked(docId: string): void {
  tracked.value = tracked.value.filter((t) => t.documentId !== docId);
  const nextProbes = { ...probes.value };
  delete nextProbes[docId];
  probes.value = nextProbes;
  void saveDesktopState();
}

/**
 * Remove every desktop-local annotation for a document (tags, tracked source,
 * probe cache, recycle-bin membership, trashed versions) in one pass and
 * persist. Called when a document is permanently deleted so no orphaned
 * metadata lingers in desktop-state.json. The document's source file on disk
 * is not touched.
 */
function clearDoc(docId: string): void {
  if (tags.value[docId]) {
    const nextTags = { ...tags.value };
    delete nextTags[docId];
    tags.value = nextTags;
  }
  if (assignments.value[docId]) {
    const nextAssign = { ...assignments.value };
    delete nextAssign[docId];
    assignments.value = nextAssign;
  }
  tracked.value = tracked.value.filter((t) => t.documentId !== docId);
  const nextProbes = { ...probes.value };
  delete nextProbes[docId];
  probes.value = nextProbes;
  if (trashed.value.includes(docId)) {
    trashed.value = trashed.value.filter((id) => id !== docId);
  }
  // The document is gone, so any of its trashed versions are moot.
  if (trashedVersions.value.some((v) => v.documentId === docId)) {
    trashedVersions.value = trashedVersions.value.filter(
      (v) => v.documentId !== docId,
    );
  }
  void saveDesktopState();
}

/**
 * Probe a path and build a tracked-file baseline from it (size + mtime + sha256
 * when the file is within the hash threshold). Used right after a commit
 * (import) or a re-specify to capture the fresh baseline.
 */
async function probeAndBaseline(
  docId: string,
  path: string,
): Promise<TrackedFile> {
  if (!isTauri()) {
    // Browser dev cannot stat real files; return a placeholder baseline. The
    // mock probes drive status for the seeded mock documents.
    return { documentId: docId, path, size: 0, mtimeMs: 0, sha256: null };
  }
  const raw = await invoke<RawFileProbe>("probe_file", {
    path,
    max_bytes: MODIFICATION_HASH_THRESHOLD_BYTES,
  });
  const probe = mapFileProbe(raw);
  return {
    documentId: docId,
    path,
    size: probe.size,
    mtimeMs: probe.mtimeMs,
    sha256: probe.sha256 ?? null,
  };
}

/**
 * Two-tier modification detection for every tracked file:
 *   1. Fast pass: batch stat_files (size + mtime, no hashing).
 *   2. Full pass: only when stat changed AND the baseline is small enough,
 *      probe_file for a sha256 to confirm. Large files trust the stat change.
 * Browser dev drives the same derive logic from the mock probe fixtures.
 * Re-entrant calls are coalesced (a poll landing mid-refresh is a no-op).
 */
async function refreshModifications(): Promise<void> {
  if (!isTauri()) {
    probes.value = { ...mockProbes };
    return;
  }
  if (refreshInFlight || tracked.value.length === 0) return;
  refreshInFlight = true;
  try {
    const paths = tracked.value.map((t) => t.path);
    const stats = await invoke<RawFileStat[]>("stat_files", { paths });
    const statByPath = new Map(stats.map((s) => [s.path, s]));
    const nextProbes: Record<string, FileProbe> = { ...probes.value };

    for (const t of tracked.value) {
      const rawStat = statByPath.get(t.path);
      if (!rawStat) continue;
      const stat: FileStat = mapFileStat(rawStat);
      if (!stat.exists) {
        nextProbes[t.documentId] = {
          exists: false,
          size: 0,
          mtimeMs: 0,
          sha256: null,
        };
        continue;
      }
      const statMatches =
        stat.size === t.size && stat.mtimeMs === t.mtimeMs;
      if (statMatches) {
        // Fast path: unchanged; no hash needed.
        nextProbes[t.documentId] = {
          exists: true,
          size: stat.size,
          mtimeMs: stat.mtimeMs,
          sha256: null,
        };
        continue;
      }
      // Stat changed: full probe only when the baseline is hashable.
      const hashable =
        t.sha256 != null && t.size <= MODIFICATION_HASH_THRESHOLD_BYTES;
      if (hashable) {
        const raw = await invoke<RawFileProbe>("probe_file", {
          path: t.path,
          max_bytes: MODIFICATION_HASH_THRESHOLD_BYTES,
        });
        nextProbes[t.documentId] = mapFileProbe(raw);
      } else {
        // Large file (no baseline sha) or missing digest: trust the stat change.
        nextProbes[t.documentId] = {
          exists: true,
          size: stat.size,
          mtimeMs: stat.mtimeMs,
          sha256: null,
        };
      }
    }
    probes.value = nextProbes;
  } finally {
    refreshInFlight = false;
  }
}

// --- pending track registry (resolved by App.vue on commit success) ---

function registerPendingTrack(jobId: string, pending: PendingTrack): void {
  pendingTracks.value = { ...pendingTracks.value, [jobId]: pending };
}

function takePendingTrack(jobId: string): PendingTrack | undefined {
  const pending = pendingTracks.value[jobId];
  if (!pending) return undefined;
  const next = { ...pendingTracks.value };
  delete next[jobId];
  pendingTracks.value = next;
  return pending;
}

export function useDesktopState() {
  return {
    // state
    tags,
    tracked,
    projects,
    assignments,
    sortPrefs,
    trashed,
    trashedVersions,
    probes,
    loaded,
    allTags,
    // load / save
    loadDesktopState,
    saveDesktopState,
    // tags
    setDocumentTags,
    addTag,
    removeTag,
    // projects
    createProject,
    renameProject,
    deleteProject,
    reparentProject,
    projectOf,
    setDocumentProject,
    clearDocumentProject,
    isAncestorOrSelf,
    projectPath,
    // sort prefs
    getSortPref,
    setSortPref,
    // recycle bin
    isTrashed,
    trashDoc,
    restoreDoc,
    trashedIds,
    // version recycle bin
    isVersionTrashed,
    trashVersion,
    restoreVersion,
    trashedVersionList,
    clearVersion,
    // tracked
    trackedPathFor,
    modificationFor,
    setTracked,
    clearTracked,
    clearDoc,
    probeAndBaseline,
    refreshModifications,
    // pending tracks
    registerPendingTrack,
    takePendingTrack,
  };
}
