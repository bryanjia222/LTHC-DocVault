import { describe, it, expect } from "vitest";
import { ref, computed, watch, nextTick } from "vue";

/*
 * Reproduces the two reactivity shapes DocumentPreview's version/document watch
 * can take, against the same input pattern that occurs in production:
 * DocumentsView's 5s modification poll rewrites `probes`, which recomputes
 * `documents` (a fresh object per doc) and thus regenerates `selectedDocument`'s
 * *reference* while its `.id` stays the same. DocumentPreview receives
 * `props.document = selectedDocument`, so that reference changes every poll.
 *
 * A getter that *returns* `[version, id]` allocates a fresh array each run; Vue
 * compares the return by identity -> fires every poll even though version/id are
 * unchanged -> load() -> refreshMutable -> the "loading latest preview" badge
 * looped. An array *of getters* compares each element by Object.is, so an
 * unchanged id/version does not fire.
 */
describe("DocumentPreview watch source shape (poll-loop root-cause)", () => {
  it("getter returning an array fires when only the doc reference changes", async () => {
    const version = ref<{ label: string } | null>(null);
    const poll = ref(0); // simulates refreshModifications rewriting probes
    const docId = ref("doc1");
    // selectedDocument: a NEW object each poll (reference changes), same id.
    const selectedDocument = computed(() => ({ id: docId.value, n: poll.value }));
    const propsDoc = computed(() => selectedDocument.value);

    let fires = 0;
    // OLD shape: single getter returning an array.
    watch(
      () => [version.value, propsDoc.value.id] as const,
      () => {
        fires++;
      },
    );

    poll.value++; // poll tick: doc reference regenerates, id unchanged
    await nextTick();

    expect(fires).toBe(1); // the loop: fired despite no version/id change
  });

  it("array of getters does NOT fire when only the doc reference changes", async () => {
    const version = ref<{ label: string } | null>(null);
    const poll = ref(0);
    const docId = ref("doc1");
    const selectedDocument = computed(() => ({ id: docId.value, n: poll.value }));
    const propsDoc = computed(() => selectedDocument.value);

    let fires = 0;
    // NEW shape: array of getters (per-element Object.is compare) + sync flush.
    watch(
      [() => version.value, () => propsDoc.value?.id],
      () => {
        fires++;
      },
      { flush: "sync" },
    );

    poll.value++;
    await nextTick();

    expect(fires).toBe(0); // fixed: id unchanged -> no fire
  });

  it("array of getters still fires on a real version switch", async () => {
    const version = ref<{ label: string } | null>(null);
    const poll = ref(0);
    const docId = ref("doc1");
    const selectedDocument = computed(() => ({ id: docId.value, n: poll.value }));
    const propsDoc = computed(() => selectedDocument.value);

    let fires = 0;
    watch(
      [() => version.value, () => propsDoc.value?.id],
      () => {
        fires++;
      },
      { flush: "sync" },
    );

    version.value = { label: "v2" }; // genuine change -> must fire
    await nextTick();

    expect(fires).toBe(1);
  });
});
