import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import {
  appUpdateProgressPercent,
  appUpdateProgressText,
} from "../lib/appUpdate";
import type { AppUpdateInfo, AppUpdateProgress } from "../types/app";

type UpdatePhase =
  | "idle"
  | "checking"
  | "available"
  | "confirming"
  | "up_to_date"
  | "installing"
  | "error";

export function AboutPage() {
  const [currentVersion, setCurrentVersion] = useState("读取中…");
  const [availableUpdate, setAvailableUpdate] = useState<AppUpdateInfo | null>(null);
  const [phase, setPhase] = useState<UpdatePhase>("idle");
  const [progress, setProgress] = useState<AppUpdateProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void desktopApi
      .getAppVersion()
      .then((version) => active && setCurrentVersion(version))
      .catch(() => active && setCurrentVersion("无法读取"));
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    if (!desktopApi.isDesktopRuntime()) {
      return;
    }
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void listen<AppUpdateProgress>("app-update-progress", (event) => {
      if (!disposed) {
        setProgress(event.payload);
      }
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        cleanup = unlisten;
      }
    });
    return () => {
      disposed = true;
      cleanup?.();
    };
  }, []);

  async function checkForUpdate() {
    setPhase("checking");
    setError(null);
    setAvailableUpdate(null);
    try {
      const update = await desktopApi.checkForAppUpdate();
      if (update) {
        setAvailableUpdate(update);
        setCurrentVersion(update.currentVersion);
        setPhase("available");
      } else {
        setPhase("up_to_date");
      }
    } catch (reason) {
      setError(String(reason));
      setPhase("error");
    }
  }

  async function installUpdate() {
    if (!availableUpdate) {
      return;
    }
    setPhase("installing");
    setProgress(null);
    setError(null);
    try {
      await desktopApi.installAppUpdate(availableUpdate.version);
    } catch (reason) {
      setError(String(reason));
      setPhase("error");
    }
  }

  const checking = phase === "checking";
  const installing = phase === "installing";
  const progressPercent = appUpdateProgressPercent(progress);

  return (
    <section className="about-page">
      <header className="page-header">
        <div>
          <p className="eyebrow">ELECTRONIC JOURNEY · DESKTOP</p>
          <h1>关于与更新</h1>
          <p>更新检查只访问本项目的 GitHub Release，不上传截图、设备标识或使用统计。</p>
        </div>
      </header>

      <div className="about-grid">
        <section className="settings-panel about-version-card">
          <div>
            <span className="about-version-card__label">当前版本</span>
            <strong>Electronic Journey {currentVersion}</strong>
            <small>macOS 与 Windows 桌面版</small>
          </div>
          <button
            aria-busy={checking}
            className="button button--primary"
            disabled={checking || installing || !desktopApi.isDesktopRuntime()}
            onClick={() => void checkForUpdate()}
            type="button"
          >
            {checking ? "正在检查…" : "检查更新"}
          </button>
        </section>

        <aside className="notice-card update-signing-notice" role="note">
          <span aria-hidden="true">!</span>
          <div>
            <strong>当前安装包没有操作系统代码签名</strong>
            <p>
              更新包仍会通过应用内置公钥验证来源和完整性，但 macOS Gatekeeper 或 Windows
              SmartScreen 仍可能显示安全警告。
            </p>
          </div>
        </aside>
      </div>

      {phase === "up_to_date" && (
        <div className="success-banner update-result" role="status">
          当前已经是最新版本。
        </div>
      )}

      {availableUpdate && ["available", "confirming", "installing"].includes(phase) && (
        <section className="settings-panel update-card" aria-live="polite">
          <header>
            <div>
              <span className="about-version-card__label">发现新版本</span>
              <h2>{availableUpdate.version}</h2>
            </div>
            <span className="update-card__version-path">
              {availableUpdate.currentVersion} → {availableUpdate.version}
            </span>
          </header>

          {availableUpdate.notes && (
            <div className="update-card__notes">
              <strong>更新说明</strong>
              <p>{availableUpdate.notes}</p>
            </div>
          )}

          {phase === "confirming" && (
            <div className="update-confirmation" role="alertdialog" aria-labelledby="update-confirm-title">
              <div>
                <strong id="update-confirm-title">确认下载并安装 {availableUpdate.version}？</strong>
                <p>安装前会停止截图；如果当前有上传任务，更新会中止并请你先处理上传。</p>
              </div>
              <div className="update-confirmation__actions">
                <button
                  className="button button--ghost"
                  onClick={() => setPhase("available")}
                  type="button"
                >
                  取消
                </button>
                <button
                  className="button button--primary"
                  onClick={() => void installUpdate()}
                  type="button"
                >
                  下载并安装
                </button>
              </div>
            </div>
          )}

          {installing ? (
            <div className="update-progress" role="status">
              <span className="update-progress__track" aria-hidden="true">
                <i
                  style={{
                    width: progress?.totalBytes
                      ? `${progressPercent ?? 0}%`
                      : "24%",
                  }}
                />
              </span>
              <strong>{appUpdateProgressText(progress)}</strong>
              <small>请保持应用运行，不要强制退出。</small>
            </div>
          ) : phase !== "confirming" ? (
            <button
              className="button button--primary"
              onClick={() => setPhase("confirming")}
              type="button"
            >
              查看安装确认
            </button>
          ) : null}
        </section>
      )}

      {error && (
        <div className="error-banner update-result" role="alert">
          <span>{error}</span>
          <button className="button button--ghost" onClick={() => void checkForUpdate()} type="button">
            重新检查
          </button>
        </div>
      )}

      <p className="about-page__privacy-note">
        只有你点击“检查更新”或“下载并安装”时才会连接 GitHub；应用不会在后台静默安装更新。
      </p>
    </section>
  );
}
