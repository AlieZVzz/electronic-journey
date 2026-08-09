import { invoke } from "@tauri-apps/api/core";

import { defaultCaptureSettings } from "../lib/settings";
import type {
  AppSnapshot,
  CaptureSettings,
  RecordingState,
  TimelinePageResult,
  TimelineSelectionItem,
  TraySnapshot,
  RemoteConnectionTest,
  RemoteProfile,
  SaveRemoteProfileInput,
  UploadBatchProgress,
} from "../types/app";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const browserSnapshot: AppSnapshot = {
  state: "stopped",
  suspensionReason: null,
  nextCaptureAt: null,
  todayCount: 0,
  localStorageBytes: 0,
  pendingUploads: 0,
  permissionGranted: false,
  permissionState: "not_determined",
  lastError: null,
  settings: defaultCaptureSettings,
  launchAtLogin: false,
};

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let permissionRefresh: Promise<AppSnapshot> | null = null;

export const desktopApi = {
  isDesktopRuntime(): boolean {
    return isTauriRuntime();
  },

  isWindowsDesktopRuntime(): boolean {
    return (
      isTauriRuntime() &&
      typeof navigator !== "undefined" &&
      navigator.userAgent.includes("Windows")
    );
  },

  initialSnapshot(): AppSnapshot {
    return structuredClone(browserSnapshot);
  },

  async getSnapshot(): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      return structuredClone(browserSnapshot);
    }

    return invoke<AppSnapshot>("get_app_snapshot");
  },

  async getTraySnapshot(): Promise<TraySnapshot> {
    if (!isTauriRuntime()) {
      return {
        state: browserSnapshot.state,
        suspensionReason: browserSnapshot.suspensionReason,
        permissionState: browserSnapshot.permissionState,
        todayCaptured: browserSnapshot.todayCount,
        todayUploaded: 0,
      };
    }
    return invoke<TraySnapshot>("get_tray_snapshot");
  },

  async openMainWindowFromTray(): Promise<void> {
    if (isTauriRuntime()) {
      await invoke<void>("open_main_window_from_tray");
    }
  },

  async quitFromTray(): Promise<void> {
    if (isTauriRuntime()) {
      await invoke<void>("quit_from_tray");
    }
  },

  async refreshScreenCapturePermission(): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      return structuredClone(browserSnapshot);
    }
    if (!permissionRefresh) {
      permissionRefresh = (async () => {
        try {
          return await invoke<AppSnapshot>(
            "refresh_screen_capture_permission",
          );
        } finally {
          permissionRefresh = null;
        }
      })();
    }
    return permissionRefresh;
  },

  async setRecordingState(state: RecordingState): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      browserSnapshot.state = state;
      browserSnapshot.suspensionReason = null;
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

  async setLaunchAtLogin(enabled: boolean): Promise<AppSnapshot> {
    if (!isTauriRuntime()) {
      throw new Error("开机自启动只能在桌面应用中设置。");
    }

    return invoke<AppSnapshot>("set_launch_at_login", { enabled });
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

  async listTimelineDaySelection(
    dateKey: string,
  ): Promise<TimelineSelectionItem[]> {
    if (!isTauriRuntime()) {
      return [];
    }

    return invoke<TimelineSelectionItem[]>(
      "list_timeline_day_selection",
      { dateKey },
    );
  },

  async readTimelineCapture(captureId: string): Promise<ArrayBuffer> {
    if (!isTauriRuntime()) {
      throw new Error("截图只能在桌面应用中查看。");
    }

    return invoke<ArrayBuffer>("read_timeline_capture", { captureId });
  },

  async readTimelineThumbnail(captureId: string): Promise<ArrayBuffer> {
    if (!isTauriRuntime()) {
      throw new Error("截图缩略图只能在桌面应用中查看。");
    }

    return invoke<ArrayBuffer>("read_timeline_thumbnail", { captureId });
  },

  async deleteTimelineCapture(captureId: string): Promise<void> {
    if (!isTauriRuntime()) {
      throw new Error("截图只能在桌面应用中删除。");
    }

    return invoke<void>("delete_timeline_capture", { captureId });
  },

  async getRemoteProfile(): Promise<RemoteProfile | null> {
    if (!isTauriRuntime()) {
      return null;
    }
    return invoke<RemoteProfile | null>("get_remote_profile");
  },

  async pickPrivateKeyFile(): Promise<string | null> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中选择 SSH 私钥文件。");
    }
    return invoke<string | null>("pick_private_key_file");
  },

  async probeRemoteHostKey(host: string, port: number): Promise<string> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中读取远程服务器指纹。");
    }
    return invoke<string>("probe_remote_host_key", { host, port });
  },

  async saveRemoteProfile(input: SaveRemoteProfileInput): Promise<RemoteProfile> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中保存远程服务器配置。");
    }
    return invoke<RemoteProfile>("save_remote_profile", { input });
  },

  async testRemoteProfile(): Promise<RemoteConnectionTest> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中测试远程服务器。");
    }
    return invoke<RemoteConnectionTest>("test_remote_profile");
  },

  async syncTodayNow(): Promise<void> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中同步截图。");
    }
    return invoke<void>("sync_today_now");
  },

  async uploadSelectedCaptures(
    captureIds: string[],
  ): Promise<UploadBatchProgress> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中上传截图。");
    }
    return invoke<UploadBatchProgress>("upload_selected_captures", {
      captureIds,
    });
  },

  async getUploadBatchStatus(
    batchId: string,
  ): Promise<UploadBatchProgress> {
    if (!isTauriRuntime()) {
      throw new Error("只能在桌面应用中读取上传状态。");
    }
    return invoke<UploadBatchProgress>("get_upload_batch_status", {
      batchId,
    });
  },

  async getActiveUploadBatch(): Promise<UploadBatchProgress | null> {
    if (!isTauriRuntime()) {
      return null;
    }
    return invoke<UploadBatchProgress | null>("get_active_upload_batch");
  },
};
