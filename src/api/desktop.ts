import { invoke } from "@tauri-apps/api/core";

import { defaultCaptureSettings } from "../lib/settings";
import type {
  AppSnapshot,
  CaptureSettings,
  RecordingState,
  TimelinePageResult,
} from "../types/app";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const browserSnapshot: AppSnapshot = {
  state: "stopped",
  nextCaptureAt: null,
  todayCount: 0,
  localStorageBytes: 0,
  pendingUploads: 0,
  cloudEnabled: false,
  permissionGranted: false,
  permissionState: "not_determined",
  lastError: null,
  settings: defaultCaptureSettings,
};

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const desktopApi = {
  isDesktopRuntime(): boolean {
    return isTauriRuntime();
  },

  async getSnapshot(): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      return structuredClone(browserSnapshot);
    }

    return invoke<AppSnapshot>("get_app_snapshot");
  },

  async setRecordingState(state: RecordingState): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      browserSnapshot.state = state;
      browserSnapshot.nextCaptureAt =
        state === "running"
          ? new Date(Date.now() + 10 * 1000).toISOString()
          : null;
      return structuredClone(browserSnapshot);
    }

    return invoke<AppSnapshot>("set_recording_state", { state });
  },

  async requestScreenCapturePermission(): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      throw new Error("屏幕录制权限只能在桌面应用中申请。");
    }

    return invoke<AppSnapshot>("request_screen_capture_permission");
  },

  async updateSettings(settings: CaptureSettings): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      browserSnapshot.settings = settings;
      if (browserSnapshot.state === "running") {
        browserSnapshot.nextCaptureAt = new Date(
          Date.now() + settings.intervalMinutes * 60 * 1000,
        ).toISOString();
      }
      return structuredClone(browserSnapshot);
    }

    return invoke<AppSnapshot>("update_capture_settings", { settings });
  },

  async listTimelineCaptures(
    offset = 0,
    limit = 18,
  ): Promise<TimelinePageResult> {
    if (!isTauriRuntime()) {
      return { items: [], nextOffset: null };
    }

    return invoke<TimelinePageResult>("list_timeline_captures", {
      offset,
      limit,
    });
  },

  async readTimelineCapture(captureId: string): Promise<ArrayBuffer> {
    if (!isTauriRuntime()) {
      throw new Error("截图只能在桌面应用中解密查看。");
    }

    return invoke<ArrayBuffer>("read_timeline_capture", { captureId });
  },
};
