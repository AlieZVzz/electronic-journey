import { useCallback, useEffect, useMemo, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { desktopApi } from "../api/desktop";
import {
  trayActionAvailability,
  trayStatusPresentation,
} from "../lib/trayPanel";
import type { RecordingState, TraySnapshot } from "../types/app";

function Icon({ name }: { name: "play" | "pause" | "stop" | "open" | "quit" | "close" }) {
  if (name === "play") {
    return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m7 5 7 5-7 5V5Z" /></svg>;
  }
  if (name === "pause") {
    return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M7 5v10M13 5v10" /></svg>;
  }
  if (name === "stop") {
    return <svg viewBox="0 0 20 20" aria-hidden="true"><rect x="6" y="6" width="8" height="8" rx="1" /></svg>;
  }
  if (name === "open") {
    return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M6 4h10v10M15.5 4.5l-7 7M14 10v6H4V6h6" /></svg>;
  }
  if (name === "quit") {
    return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="M10 3v7M5.8 5.7A7 7 0 1 0 14.2 5.7" /></svg>;
  }
  return <svg viewBox="0 0 20 20" aria-hidden="true"><path d="m6 6 8 8M14 6l-8 8" /></svg>;
}

export function TrayPanel() {
  const [snapshot, setSnapshot] = useState<TraySnapshot | null>(null);
  const [pendingState, setPendingState] = useState<RecordingState | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await desktopApi.getTraySnapshot());
      setError(null);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    void refresh();
    if (!desktopApi.isDesktopRuntime()) {
      return;
    }
    let active = true;
    const unlisteners: UnlistenFn[] = [];

    Promise.all([
      listen("tray-panel-opened", () => void refresh()),
      listen("runtime-state-changed", () => void refresh()),
    ]).then((listeners) => {
      if (active) {
        unlisteners.push(...listeners);
      } else {
        listeners.forEach((unlisten) => unlisten());
      }
    });

    return () => {
      active = false;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [refresh]);

  const presentation = useMemo(
    () =>
      trayStatusPresentation({
        state: snapshot?.state ?? "stopped",
        suspensionReason: snapshot?.suspensionReason ?? null,
      }),
    [snapshot?.state, snapshot?.suspensionReason],
  );
  const actions = trayActionAvailability(snapshot?.state ?? "stopped");

  async function hidePanel() {
    try {
      await getCurrentWindow().hide();
    } catch {
      // The panel may already be hidden after focus moved elsewhere.
    }
  }

  async function openMainWindow() {
    await desktopApi.openMainWindowFromTray();
    await hidePanel();
  }

  async function setRecordingState(state: RecordingState) {
    if (state === "running" && snapshot?.permissionState !== "granted") {
      await openMainWindow();
      return;
    }
    setPendingState(state);
    setError(null);
    try {
      await desktopApi.setRecordingState(state);
      await refresh();
      await hidePanel();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPendingState(null);
    }
  }

  return (
    <main className="tray-panel" aria-label="Electronic Journey 托盘控制面板">
      <header className="tray-panel__header">
        <div className="tray-panel__brand-mark" aria-hidden="true">
          <i />
          <i />
          <i />
        </div>
        <div>
          <strong>Electronic Journey</strong>
          <span>本地数字旅程</span>
        </div>
        <button className="tray-panel__close" onClick={() => void hidePanel()} type="button" aria-label="关闭面板">
          <Icon name="close" />
        </button>
      </header>

      <section className={`tray-panel__status is-${presentation.tone}`}>
        <span className="tray-panel__status-dot" aria-hidden="true" />
        <div>
          <strong>{presentation.label}</strong>
          <p>{presentation.detail}</p>
        </div>
      </section>

      {snapshot?.permissionState !== "granted" && (
        <button className="tray-panel__permission" onClick={() => void openMainWindow()} type="button">
          <span>!</span>
          <div>
            <strong>需要屏幕录制权限</strong>
            <small>在主窗口阅读说明并完成授权</small>
          </div>
          <b aria-hidden="true">›</b>
        </button>
      )}

      <section className="tray-panel__metrics" aria-label="今日统计">
        <div>
          <span>今日截图</span>
          <strong>{snapshot?.todayCaptured ?? "—"}</strong>
          <small>张</small>
        </div>
        <div>
          <span>今日已上传</span>
          <strong>{snapshot?.todayUploaded ?? "—"}</strong>
          <small>张</small>
        </div>
      </section>

      <section className="tray-panel__actions" aria-label="记录控制">
        <button
          className="tray-panel__action tray-panel__action--primary"
          disabled={!actions.start || pendingState !== null}
          onClick={() => void setRecordingState("running")}
          type="button"
        >
          <Icon name="play" />
          <span>{pendingState === "running" ? "正在开始…" : snapshot?.state === "paused" ? "继续记录" : "开始记录"}</span>
        </button>
        <button disabled={!actions.pause || pendingState !== null} onClick={() => void setRecordingState("paused")} type="button">
          <Icon name="pause" />
          <span>{pendingState === "paused" ? "正在暂停…" : "暂停"}</span>
        </button>
        <button disabled={!actions.stop || pendingState !== null} onClick={() => void setRecordingState("stopped")} type="button">
          <Icon name="stop" />
          <span>{pendingState === "stopped" ? "正在停止…" : "停止"}</span>
        </button>
      </section>

      {error && <p className="tray-panel__error" role="alert">{error}</p>}

      <footer className="tray-panel__footer">
        <button onClick={() => void openMainWindow()} type="button">
          <Icon name="open" />
          <span>打开主窗口</span>
          <kbd>↗</kbd>
        </button>
        <button className="tray-panel__quit" onClick={() => void desktopApi.quitFromTray()} type="button">
          <Icon name="quit" />
          <span>退出应用</span>
        </button>
      </footer>
    </main>
  );
}
