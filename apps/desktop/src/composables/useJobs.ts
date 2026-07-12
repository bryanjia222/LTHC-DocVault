import { computed } from "vue";
import { useVault } from "./useVault";

/*
 * Job list state. Shared so the metrics bar and jobs view stay consistent. The
 * list is owned by useVault (empty under Tauri until Phase 2 wires the job
 * runner; mock fixtures only in browser dev).
 */

const { jobs } = useVault();

export function useJobs() {
  const activeJobCount = computed(
    () => jobs.value.filter((job) => job.status !== "done").length,
  );

  return { jobs, activeJobCount };
}
