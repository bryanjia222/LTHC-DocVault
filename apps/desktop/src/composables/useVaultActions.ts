import { useAppActions } from "./actions/useAppActions";
import { useDocumentActions } from "./actions/useDocumentActions";
import { useTrashActions } from "./actions/useTrashActions";

/*
 * Centralized action handlers, split by domain:
 * - useDocumentActions: commit / import / export / open / rename / note +
 *   the runAction dispatch table that drives them
 * - useTrashActions: recycle-bin deletes + restores (documents and versions)
 * - useAppActions: navigation, theme, manual refresh, dev stage reset
 *
 * The three slices are merged back into a single object so consumers keep the
 * same `useVaultActions()` surface they have always used; every slice reads the
 * same module-level singletons (useVault / useDocuments / useDesktopState /
 * useDialogs), so state stays shared across them.
 */

export function useVaultActions() {
  return {
    ...useDocumentActions(),
    ...useTrashActions(),
    ...useAppActions(),
  };
}
