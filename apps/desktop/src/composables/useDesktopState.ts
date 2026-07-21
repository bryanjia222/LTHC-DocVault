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
  type TrackedFile,
} from "../data/mock";
import {
  mapDesktopState,
  mapFileProbe,
  mapFileStat,
  type RawDesktopState,
  type RawFileProbe,
  type RawFileStat,
  type RawTrackedFile,
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
/** documentId -> projectId; single membership (a doc lives in one project). */
const assignments: Ref<Record<string, string>> = ref({});
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

async function saveDesktopState(): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("set_desktop_state", {
      tags: tags.value,
      tracked: tracked.value.map(toRawTrackedFile),
      projects: projects.value,
      assignments: assignments.value,
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
 * Create a project folder and persist it, returning the new id. Rejects empty /
 * whitespace names and names already in use (case-insensitive) by returning
 * null, so the caller can surface a validation error.
 */
function createProject(name: string): string | null {
  const clean = name.trim();
  if (!clean) return null;
  const lower = clean.toLowerCase();
  if (projects.value.some((p) => p.name.toLowerCase() === lower)) return null;
  const id = makeProjectId();
  projects.value = [...projects.value, { id, name: clean }];
  void saveDesktopState();
  return id;
}

/** Rename a project. Returns false (no-op) on empty name, a name taken by
 * another project, or an unknown id. */
function renameProject(id: string, name: string): boolean {
  const clean = name.trim();
  if (!clean) return false;
  const lower = clean.toLowerCase();
  if (projects.value.some((p) => p.id !== id && p.name.toLowerCase() === lower)) {
    return false;
  }
  if (!projects.value.some((p) => p.id === id)) return false;
  projects.value = projects.value.map((p) => (p.id === id ? { ...p, name: clean } : p));
  void saveDesktopState();
  return true;
}

/**
 * Delete a project folder. Documents assigned to it are not deleted - they fall
 * back to the ungrouped "all documents" list (their assignment is cleared).
 */
function deleteProject(id: string): void {
  if (!projects.value.some((p) => p.id === id)) return;
  projects.value = projects.value.filter((p) => p.id !== id);
  const next: Record<string, string> = {};
  for (const [docId, projId] of Object.entries(assignments.value)) {
    if (projId !== id) next[docId] = projId;
  }
  assignments.value = next;
  void saveDesktopState();
}

/** The project id a document is assigned to, or null when ungrouped. */
function projectFor(docId: string): string | null {
  return assignments.value[docId] ?? null;
}

/** Assign a document to a project, or unassign it (projectId null / unknown).
 * Persisted. */
function setDocumentProject(docId: string, projectId: string | null): void {
  const next = { ...assignments.value };
  if (projectId && projects.value.some((p) => p.id === projectId)) {
    next[docId] = projectId;
  } else {
    delete next[docId];
  }
  assignments.value = next;
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
 * probe cache) in one pass and persist. Called when a document is deleted so no
 * orphaned metadata lingers in desktop-state.json. The document's source file
 * on disk is not touched (delete only "unmanages" the document).
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
    projectFor,
    setDocumentProject,
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
