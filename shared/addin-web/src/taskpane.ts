import "./taskpane.css";
import { bridge, BridgeError, logTaskPaneError } from "./bridge";
import { HostAdapter, MAX_BYTES, TooLargeError } from "./host";

export interface TaskPaneOptions {
  /** Shown on the big save button; defaults to "保存到 DocVault". */
  saveLabel?: string;
}

/** Mount the task-pane UI into `root` and drive it with `host`. Renders the
 *  shell synchronously, then runs a health check + document-list load; the
 *  returned promise resolves once that initial load has settled so callers can
 *  await readiness (used by the plain-browser dev page).
 */
export async function mountTaskPane(
  host: HostAdapter,
  root: HTMLElement,
  options: TaskPaneOptions = {},
): Promise<void> {
  root.innerHTML = `
    <div class="status" id="status">连接 DocVault…</div>
    <form id="form" hidden>
      <div class="mode">
        <label><input type="radio" name="mode" value="new" checked /> 新增文档</label>
        <label><input type="radio" name="mode" value="version" /> 提交新版本</label>
      </div>
      <div class="field" id="target-field" hidden>
        <label for="target">目标文档</label>
        <select id="target"></select>
      </div>
      <div class="field" id="note-field" hidden>
        <label for="note">备注</label>
        <input id="note" type="text" placeholder="本次修改说明（可选）" />
      </div>
      <button id="save" type="submit" disabled>${options.saveLabel ?? "保存到 DocVault"}</button>
      <div class="result" id="result" role="status"></div>
    </form>
  `;

  const status = root.querySelector<HTMLDivElement>("#status")!;
  const form = root.querySelector<HTMLFormElement>("#form")!;
  const save = root.querySelector<HTMLButtonElement>("#save")!;
  const result = root.querySelector<HTMLDivElement>("#result")!;
  const targetField = root.querySelector<HTMLDivElement>("#target-field")!;
  const noteField = root.querySelector<HTMLDivElement>("#note-field")!;
  const target = root.querySelector<HTMLSelectElement>("#target")!;
  const note = root.querySelector<HTMLInputElement>("#note")!;
  const modeRadios = Array.from(root.querySelectorAll<HTMLInputElement>('input[name="mode"]'));

  const setStatus = (text: string, kind: "" | "ok" | "error" = "") => {
    status.textContent = text;
    status.className = `status ${kind}`;
  };

  const isVersionMode = () =>
    (form.elements.namedItem("mode") as RadioNodeList).value === "version";

  const updateMode = () => {
    const isVersion = isVersionMode();
    targetField.hidden = !isVersion;
    noteField.hidden = !isVersion;
  };
  for (const radio of modeRadios) radio.addEventListener("change", updateMode);

  // Initial connection check + document list for the target picker.
  try {
    const health = await bridge.health();
    if (!health.ok || !health.vaultOpen) {
      setStatus("DocVault 未运行或尚未打开仓库。请先打开 DocVault 并连接仓库。", "error");
      logTaskPaneError(
        "taskpane.health",
        "DocVault is not running or the vault is not open",
      );
      return;
    }
    setStatus(`已连接 DocVault v${health.version}`, "ok");
    const { documents } = await bridge.listDocuments();
    for (const doc of documents) {
      const option = document.createElement("option");
      option.value = doc.id;
      option.textContent = doc.name;
      target.append(option);
    }
    save.disabled = false;
    form.hidden = false;
  } catch (e) {
    setStatus(friendly(e), "error");
    if (!(e instanceof BridgeError)) logTaskPaneError("taskpane.initialize", e);
    return;
  }

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    save.disabled = true;
    result.textContent = "正在读取文档…";
    try {
      const doc = await host.getCurrentDocument();
      if (isVersionMode()) {
        if (!target.value) throw new Error("请选择目标文档");
        result.textContent = "正在保存为文档新版本…";
        const noteText = note.value.trim() || undefined;
        const { jobId } = await bridge.commitVersion(target.value, doc.ext, doc.bytes, noteText);
        result.textContent = `已提交新版本（任务 ${jobId}），DocVault 将在后台压缩。`;
      } else {
        result.textContent = "正在保存为新文档…";
        const { documentId } = await bridge.importDocument(
          `${doc.name}.${doc.ext}`,
          doc.ext,
          doc.bytes,
        );
        result.textContent = `已保存为新文档（${documentId}）。`;
      }
      setStatus("保存成功", "ok");
    } catch (e) {
      result.textContent = friendly(e);
      if (e instanceof TooLargeError || !(e instanceof BridgeError)) {
        logTaskPaneError("taskpane.save", e);
      }
    } finally {
      save.disabled = false;
    }
  });
}

function friendly(error: unknown): string {
  if (error instanceof TooLargeError) {
    return `文档超过 ${Math.round(MAX_BYTES / 1024 / 1024)}MB，无法经插件保存。请在 DocVault 中使用「添加文档」手动导入。`;
  }
  if (error instanceof BridgeError) {
    return `DocVault 桥接错误：${error.message}`;
  }
  return error instanceof Error ? error.message : String(error);
}
