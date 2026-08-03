import { useCallback, useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { desktopApi } from "../api/desktop";
import {
  recordingSuccessMessage,
  type RuntimeAction,
} from "../lib/interactionFeedback";
import type {
  AppSnapshot,
  CaptureSettings,
  RecordingState,
} from "../types/app";

interface AppRuntime {
  snapshot: AppSnapshot | null;
  loading: boolean;
  pendingAction: RuntimeAction | null;
  recordingTarget: RecordingState | null;
  error: string | null;
  notice: string | null;
  dismissNotice: () => void;
  requestScreenCapturePermission: () => Promise<AppSnapshot>;
  setRecordingState: (state: RecordingState) => Promise<void>;
  updateSettings: (settings: CaptureSettings) => Promise<void>;
  setLaunchAtLogin: (enabled: boolean) => Promise<void>;
}

export function useAppRuntime(): AppRuntime {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(() =>
    desktopApi.initialSnapshot(),
  );
  const [loading, setLoading] = useState(true);
  const [pendingAction, setPendingAction] = useState<RuntimeAction | null>(null);
  const [recordingTarget, setRecordingTarget] =
    useState<RecordingState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    async function loadRuntime() {
      try {
        const cachedSnapshot = await desktopApi.getSnapshot();
        if (active) {
          setSnapshot(cachedSnapshot);
          setError(null);
        }

        if (!desktopApi.isDesktopRuntime()) {
          return;
        }
        const refreshedSnapshot =
          await desktopApi.refreshScreenCapturePermission();
        if (active) {
          setSnapshot(refreshedSnapshot);
          setError(null);
        }
      } catch (reason: unknown) {
        if (active) {
          setError(String(reason));
        }
      } finally {
        if (active) {
          setLoading(false);
        }
      }
    }

    void loadRuntime();

    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!desktopApi.isDesktopRuntime()) {
      return;
    }
    let active = true;
    let unlisten: UnlistenFn | undefined;
    const refreshSnapshot = () => {
      desktopApi
        .getSnapshot()
        .then((value) => {
          if (active) {
            setSnapshot(value);
            setError(null);
          }
        })
        .catch((reason: unknown) => {
          if (active) {
            setError(String(reason));
          }
        });
    };

    void listen("runtime-state-changed", refreshSnapshot).then(
      (stopListening) => {
        if (active) {
          unlisten = stopListening;
        } else {
          stopListening();
        }
      },
    );

    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!desktopApi.isDesktopRuntime()) {
      return;
    }

    const refreshPermission = () => {
      desktopApi
        .refreshScreenCapturePermission()
        .then((value) => {
          setSnapshot(value);
          setError(null);
        })
        .catch((reason: unknown) => setError(String(reason)));
    };
    const refreshSnapshot = () => {
      desktopApi
        .getSnapshot()
        .then((value) => {
          setSnapshot(value);
          setError(null);
        })
        .catch((reason: unknown) => setError(String(reason)));
    };
    const refreshPermissionWhenVisible = () => {
      if (document.visibilityState === "visible") {
        refreshPermission();
      }
    };

    window.addEventListener("focus", refreshPermission);
    window.addEventListener(
      "electronic-journey:snapshot-changed",
      refreshSnapshot,
    );
    document.addEventListener(
      "visibilitychange",
      refreshPermissionWhenVisible,
    );

    return () => {
      window.removeEventListener("focus", refreshPermission);
      window.removeEventListener(
        "electronic-journey:snapshot-changed",
        refreshSnapshot,
      );
      document.removeEventListener(
        "visibilitychange",
        refreshPermissionWhenVisible,
      );
    };
  }, []);

  useEffect(() => {
    if (
      !desktopApi.isDesktopRuntime() ||
      !["running", "suspended"].includes(snapshot?.state ?? "")
    ) {
      return;
    }

    const timer = window.setInterval(() => {
      desktopApi
        .getSnapshot()
        .then((value) => {
          setSnapshot(value);
          setError(null);
        })
        .catch((reason: unknown) => setError(String(reason)));
    }, 1000);

    return () => window.clearInterval(timer);
  }, [snapshot?.state]);

  useEffect(() => {
    if (
      !desktopApi.isDesktopRuntime() ||
      ["running", "suspended"].includes(snapshot?.state ?? "")
    ) {
      return;
    }
    const timer = window.setInterval(() => {
      desktopApi
        .getSnapshot()
        .then((value) => {
          setSnapshot(value);
          setError(null);
        })
        .catch((reason: unknown) => setError(String(reason)));
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [snapshot?.state]);

  const setRecordingState = useCallback(async (state: RecordingState) => {
    setLoading(true);
    setPendingAction("recording");
    setRecordingTarget(state);
    setNotice(null);
    try {
      setSnapshot(await desktopApi.setRecordingState(state));
      setError(null);
      setNotice(recordingSuccessMessage(state));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setRecordingTarget(null);
      setPendingAction(null);
      setLoading(false);
    }
  }, []);

  const requestScreenCapturePermission = useCallback(async () => {
    setLoading(true);
    setPendingAction("permission");
    setNotice(null);
    try {
      const nextSnapshot = await desktopApi.requestScreenCapturePermission();
      setSnapshot(nextSnapshot);
      setError(null);
      return nextSnapshot;
    } catch (reason) {
      setError(String(reason));
      throw reason;
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, []);

  const updateSettings = useCallback(async (settings: CaptureSettings) => {
    setLoading(true);
    setPendingAction("settings");
    setNotice(null);
    try {
      setSnapshot(await desktopApi.updateSettings(settings));
      setError(null);
      setNotice("设置已保存，并已应用到后续截图。");
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, []);

  const setLaunchAtLogin = useCallback(async (enabled: boolean) => {
    setLoading(true);
    setPendingAction("autostart");
    setNotice(null);
    try {
      setSnapshot(await desktopApi.setLaunchAtLogin(enabled));
      setError(null);
      setNotice(
        enabled
          ? "开机自启动已开启；下次登录系统时将在后台启动应用。"
          : "开机自启动已关闭。",
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPendingAction(null);
      setLoading(false);
    }
  }, []);

  return {
    snapshot,
    loading,
    pendingAction,
    recordingTarget,
    error,
    notice,
    dismissNotice: () => setNotice(null),
    requestScreenCapturePermission,
    setRecordingState,
    updateSettings,
    setLaunchAtLogin,
  };
}
