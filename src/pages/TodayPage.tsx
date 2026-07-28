import { useState } from "react";

import { MetricCard } from "../components/MetricCard";
import { ScreenCaptureDisclosure } from "../components/ScreenCaptureDisclosure";
import { StatusPill } from "../components/StatusPill";
import { canRequestScreenCapturePermission } from "../lib/screenCaptureDisclosure";
import type { AppSnapshot, RecordingState } from "../types/app";

interface TodayPageProps {
  snapshot: AppSnapshot;
  loading: boolean;
  onPermissionRequest: () => Promise<AppSnapshot>;
  onStateChange: (state: RecordingState) => Promise<void>;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) {
    return "0 MB";
  }

  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatNextCapture(value: string | null): string {
  if (!value) {
    return "尚未安排";
  }

  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
}

export function TodayPage({
  snapshot,
  loading,
  onPermissionRequest,
  onStateChange,
}: TodayPageProps) {
  const isRunning = snapshot.state === "running";
  const [showPermissionDisclosure, setShowPermissionDisclosure] =
    useState(false);
  const [permissionAcknowledged, setPermissionAcknowledged] = useState(false);

  function closePermissionDisclosure() {
    if (loading) {
      return;
    }

    setShowPermissionDisclosure(false);
    setPermissionAcknowledged(false);
  }

  async function requestPermissionAfterDisclosure() {
    if (
      !canRequestScreenCapturePermission(permissionAcknowledged, loading)
    ) {
      return;
    }

    try {
      const nextSnapshot = await onPermissionRequest();
      if (nextSnapshot.permissionGranted) {
        setShowPermissionDisclosure(false);
        setPermissionAcknowledged(false);
      }
    } catch {
      // The shared runtime displays the request error while this dialog stays
      // open so the person can retry or cancel.
    }
  }

  return (
    <>
      <header className="page-header">
        <div>
          <h1>今天</h1>
          <p>
            {new Intl.DateTimeFormat("zh-CN", {
              year: "numeric",
              month: "long",
              day: "numeric",
              weekday: "long",
            }).format(new Date())}
          </p>
        </div>
        <StatusPill state={snapshot.state} />
      </header>

      <section className="hero-card">
        <span
          aria-hidden="true"
          className={`capture-orb capture-orb--${snapshot.state}`}
        >
          <i />
        </span>
        <div className="hero-card__copy">
          <h2>{isRunning ? "正在记录你的数字旅程" : "记录已暂停"}</h2>
          <p>
            {isRunning
              ? `预计 ${formatNextCapture(snapshot.nextCaptureAt)} 捕获当前主显示器。`
              : `开启后 10 秒完成首次截图，随后每 ${snapshot.settings.intervalMinutes} 分钟记录一次。`}
          </p>
        </div>
        <div className="hero-card__actions">
          <button
            className="button button--primary"
            disabled={loading}
            onClick={() => onStateChange(isRunning ? "paused" : "running")}
            type="button"
          >
            {isRunning ? "暂停记录" : "开始记录"}
          </button>
          {snapshot.state !== "stopped" && (
            <button
              className="button button--ghost"
              disabled={loading}
              onClick={() => onStateChange("stopped")}
              type="button"
            >
              停止
            </button>
          )}
        </div>
      </section>

      <section className="metrics" aria-label="今日概览">
        <MetricCard
          detail="按当前展示时区统计"
          label="今日记录"
          tone="sage"
          value={`${snapshot.todayCount} 张`}
        />
        <MetricCard
          detail="本地加密保险箱"
          label="占用空间"
          value={formatBytes(snapshot.localStorageBytes)}
        />
        <MetricCard
          detail={snapshot.cloudEnabled ? "等待安全上传" : "仅本地模式"}
          label="待上传"
          tone="amber"
          value={`${snapshot.pendingUploads} 项`}
        />
      </section>

      <section className="detail-grid">
        <article className="panel">
          <div className="panel__heading">
            <div>
              <h3>最近一张截图</h3>
            </div>
            <span>{snapshot.todayCount ? "本机解密" : "暂无记录"}</span>
          </div>
          <div className="empty-canvas">
            <span aria-hidden="true">＋</span>
            <strong>你的第一段旅程会出现在这里</strong>
            <p>缩略图只在本机按需解密和生成。</p>
          </div>
        </article>

        <article className="panel panel--compact">
          <div className="panel__heading">
            <div>
              <h3>记录前检查</h3>
            </div>
          </div>
          <ul className="check-list">
            <li className={snapshot.permissionGranted ? "is-ready" : ""}>
              <span />
              <div>
                <strong>屏幕录制权限</strong>
                <p>
                  {snapshot.permissionGranted
                    ? "已授权"
                    : snapshot.permissionState === "denied"
                      ? "未授权，请检查系统设置"
                      : "尚未授权"}
                </p>
                {!snapshot.permissionGranted && (
                  <button
                    className="button button--ghost permission-button"
                    disabled={loading}
                    onClick={() => setShowPermissionDisclosure(true)}
                    type="button"
                  >
                    检查并请求权限
                  </button>
                )}
              </div>
            </li>
            <li className="is-ready">
              <span />
              <div>
                <strong>空闲暂停</strong>
                <p>{snapshot.settings.idlePauseMinutes} 分钟后暂停</p>
              </div>
            </li>
            <li className="is-ready">
              <span />
              <div>
                <strong>本地加密</strong>
                <p>XChaCha20-Poly1305</p>
              </div>
            </li>
          </ul>
        </article>
      </section>

      {showPermissionDisclosure && (
        <div className="permission-dialog-backdrop">
          <section
            aria-labelledby="permission-dialog-title"
            aria-modal="true"
            className="permission-dialog"
            role="dialog"
          >
            <p className="eyebrow">BEFORE SYSTEM PERMISSION</p>
            <h2 id="permission-dialog-title">授权前，请先了解访问方式</h2>
            <p className="permission-dialog__lead">
              确认后才会调用 macOS 的系统权限接口。
            </p>
            <ScreenCaptureDisclosure
              acknowledged={permissionAcknowledged}
              onAcknowledgementChange={setPermissionAcknowledged}
            />
            <div className="onboarding-actions">
              <button
                className="button button--onboarding-secondary"
                disabled={loading}
                onClick={closePermissionDisclosure}
                type="button"
              >
                取消
              </button>
              <button
                className="button button--onboarding-primary"
                disabled={
                  !canRequestScreenCapturePermission(
                    permissionAcknowledged,
                    loading,
                  )
                }
                onClick={() => void requestPermissionAfterDisclosure()}
                type="button"
              >
                {loading ? "正在检查…" : "我理解，打开系统授权"}
              </button>
            </div>
          </section>
        </div>
      )}
    </>
  );
}
