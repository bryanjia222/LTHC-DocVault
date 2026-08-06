import { detectHost } from "./hosts";
import { mountTaskPane } from "./taskpane";

const root = document.getElementById("app");
if (!root) throw new Error("#app element missing");

try {
  const host = detectHost();
  void mountTaskPane(host, root);
} catch (e) {
  root.innerHTML = `<div class="status error">${
    e instanceof Error ? e.message : String(e)
  }</div>`;
}
