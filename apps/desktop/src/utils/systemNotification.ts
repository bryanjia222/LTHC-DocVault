import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

import { useSystemNotificationPref } from "../composables/useSystemNotificationPref";
import { reportError } from "./reportError";
import { isTauri } from "./runtime";

let permissionDeniedForSession = false;

/**
 * Best-effort system notification delivery. Permission is requested lazily on
 * the first incoming message so startup is never blocked by an OS prompt.
 */
export async function showSystemNotification(
  title: string,
  body: string,
): Promise<void> {
  const { systemNotificationsEnabled } = useSystemNotificationPref();
  if (
    !isTauri() ||
    !systemNotificationsEnabled.value ||
    permissionDeniedForSession
  ) {
    return;
  }

  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (!granted) {
      permissionDeniedForSession = true;
      return;
    }
    sendNotification({ title, body });
  } catch (error) {
    reportError("systemNotifications.show", error);
  }
}
