import { detectHost } from "./hosts";
import { installGlobalErrorReporting, logTaskPaneError } from "./bridge";
import { mountTaskPane } from "./taskpane";

const root = document.getElementById("app");
installGlobalErrorReporting();
if (!root) {
  logTaskPaneError("taskpane.mount", "#app element missing");
  throw new Error("#app element missing");
}

try {
  const host = detectHost();
  void mountTaskPane(host, root);
} catch (e) {
  logTaskPaneError("taskpane.mount", e);
  root.innerHTML = `<div class="status error">${
    e instanceof Error ? e.message : String(e)
  }</div>`;
}
