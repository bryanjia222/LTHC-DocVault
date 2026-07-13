import type {
  FileProbe,
  ModificationStatus,
  TrackedFile,
} from "../data/mock";

/*
 * Pure modification-tracking logic, extracted from the reactive/Tauri layer so
 * the two-tier detection rules are unit-testable in isolation. The orchestration
 * (when to stat vs. hash) lives in useDesktopState; this module answers the
 * single question: given a tracked baseline and a fresh probe, what is the
 * document's modification status?
 *
 * Detection rules:
 * - No tracked file            -> "none"      (not imported on this machine)
 * - Not yet probed             -> "unchanged" (baseline was just captured)
 * - Probe says file is gone    -> "missing"   (deleted/moved -> re-specify)
 * - Stat (size+mtime) matches  -> "unchanged" (fast path; no hash needed)
 * - Stat changed, both sha256  -> compare     (full path; equal => unchanged)
 * - Stat changed, no sha256    -> "modified"  (large file or absent digest;
 *                                             can't confirm equality, trust stat)
 */

/**
 * Files at or below this size are sha256-hashed (at import time and on a full
 * detection pass). Larger files skip hashing and rely on stat alone, so a big
 * .pptx is never re-hashed on every poll.
 */
export const MODIFICATION_HASH_THRESHOLD_BYTES = 50 * 1024 * 1024;

/**
 * Derive a document's modification status from its tracked baseline and the
 * latest probe. See the module-level rules above. Pure: no I/O, no clocks.
 */
export function deriveModificationStatus(
  tracked: TrackedFile | undefined | null,
  probe: FileProbe | undefined | null,
): ModificationStatus {
  if (!tracked) return "none";
  if (!probe) return "unchanged";
  if (!probe.exists) return "missing";

  const statMatches =
    probe.size === tracked.size && probe.mtimeMs === tracked.mtimeMs;
  if (statMatches) return "unchanged";

  // Stat changed: confirm with sha256 when both sides have one. If either is
  // absent (file above the hash threshold, or digest not yet computed) we cannot
  // confirm equality, so trust the stat change and report "modified".
  if (tracked.sha256 && probe.sha256) {
    return tracked.sha256 === probe.sha256 ? "unchanged" : "modified";
  }
  return "modified";
}
