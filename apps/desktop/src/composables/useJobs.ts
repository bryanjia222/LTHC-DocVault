import { computed, ref } from "vue";
import { jobs as mockJobs } from "../data/mock";

/*
 * Job list state. Shared so the metrics bar and jobs view stay consistent.
 */

const jobs = ref([...mockJobs]);

export function useJobs() {
  const activeJobCount = computed(
    () => jobs.value.filter((job) => job.status !== "done").length,
  );

  return { jobs, activeJobCount };
}
