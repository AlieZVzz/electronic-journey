import { useCallback, useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import type {
  AppSnapshot,
  CaptureSettings,
  RecordingState,
} from "../types/app";

interface AppRuntime {
  snapshot: AppSnapshot | null;
  loading: boolean;
  error: string | null;
  requestScreenCapturePermission: () => Promise<AppSnapshot>;
  setRecordingState: (state: RecordingState) => Promise<void>;
  updateSettings: (settings: CaptureSettings) => Promise<void>;
}

export function useAppRuntime(): AppRuntime {
  const [snapshot, setSnapshot] = useState<AppSnapshot | null>(() =>
    desktopApi.initialSnapshot(),
  );
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
    if (!desktopApi.isDesktopRuntime() || snapshot?.state !== "running") {
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

  const setRecordingState = useCallback(async (state: RecordingState) => {
    setLoading(true);
    try {
      setSnapshot(await desktopApi.setRecordingState(state));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  const requestScreenCapturePermission = useCallback(async () => {
    setLoading(true);
    try {
      const nextSnapshot = await desktopApi.requestScreenCapturePermission();
      setSnapshot(nextSnapshot);
      setError(null);
      return nextSnapshot;
    } catch (reason) {
      setError(String(reason));
      throw reason;
    } finally {
      setLoading(false);
    }
  }, []);

  const updateSettings = useCallback(async (settings: CaptureSettings) => {
    setLoading(true);
    try {
      setSnapshot(await desktopApi.updateSettings(settings));
      setError(null);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  return {
    snapshot,
    loading,
    error,
    requestScreenCapturePermission,
    setRecordingState,
    updateSettings,
  };
}
