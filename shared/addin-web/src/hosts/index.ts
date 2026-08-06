import { HostAdapter } from "../host";
import { officeHost } from "./office";
import { wpsHost } from "./wps";

/** Pick the adapter for the host this task pane is running inside. Office.js
 *  exposes the global `Office`; WPS exposes a global `wps` object.
 */
export function detectHost(): HostAdapter {
  if (typeof Office !== "undefined" && Office.context?.document) {
    return officeHost;
  }
  const wpsGlobal = (globalThis as { wps?: unknown }).wps;
  if (typeof wpsGlobal !== "undefined") {
    return wpsHost;
  }
  throw new Error("未检测到 Office 或 WPS 宿主");
}
