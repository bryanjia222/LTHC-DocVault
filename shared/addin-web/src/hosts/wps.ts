import { HostAdapter } from "../host";

/** WPS JSAPI add-in (Phase 2). WPS cannot stream the active document as bytes
 *  the way Office.js can (its read APIs return text/JSON, not the file), so it
 *  will SaveAs to a temp path and tell the bridge the path instead - which also
 *  removes the 20MB cap. Until that lands, running inside WPS shows a clear
 *  "二期提供" message via the task pane's error path.
 */
export const wpsHost: HostAdapter = {
  async getCurrentDocument(): Promise<never> {
    throw new Error("WPS 插件支持将在二期提供");
  },
};
