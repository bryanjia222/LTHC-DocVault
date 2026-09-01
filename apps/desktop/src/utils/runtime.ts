/** Runtime probe shared by the vault bridge and error reporter. */
export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}
