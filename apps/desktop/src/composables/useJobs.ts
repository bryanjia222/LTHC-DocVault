import { computed } from "vue";
import { useVault } from "./useVault";

/*
 * Job list state. Shared so the metrics bar and jobs view stay consistent. The
 * list is owned by useVault, which mirrors the backend's authoritative job
 * registry via `job:update` events (mock fixtures only in browser dev).
 */

const { jobs } = useVault();

export function useJobs() {
  // Only `running` jobs count as active; succeeded/failed are terminal.
  const activeJobCount = computed(
    () => jobs.value.filter((job) => job.status === "running").length,
  );

  return { jobs, activeJobCount };
}
