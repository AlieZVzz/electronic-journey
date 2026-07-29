import { useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import { MetricCard } from "../components/MetricCard";
import { ScreenCaptureDisclosure } from "../components/ScreenCaptureDisclosure";
import { StatusPill } from "../components/StatusPill";
import { canRequestScreenCapturePermission } from "../lib/screenCaptureDisclosure";
import type {
  AppSnapshot,
  RecordingState,
  TimelineCapture,
} from "../types/app";

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
  const [recentCapture, setRecentCapture] = useState<{
    capture: TimelineCapture;
    url: string;
  } | null>(null);
  const [recentLoading, setRecentLoading] = useState(true);
  const [recentError, setRecentError] = useState(false);

  useEffect(() => {
    let active = true;
    let objectUrl: string | null = null;
    setRecentCapture(null);
    setRecentLoading(true);
    setRecentError(false);

    void desktopApi
      .listTimelineCaptures(0, 1)
      .then(async (page) => {
        const capture = page.items[0];
        if (!capture) {
          if (active) {
            setRecentCapture(null);
          }
          return;
        }
        const bytes = await desktopApi.readTimelineCapture(capture.id);
        if (!active) {
          return;
        }
        objectUrl = URL.createObjectURL(
          new Blob([bytes], { type: "image/webp" }),
        );
        setRecentCapture({ capture, url: objectUrl });
      })
      .catch(() => {
        if (active) {
          setRecentCapture(null);
          setRecentError(true);
        }
      })
      .finally(() => {
        if (active) {
          setRecentLoading(false);
        }
      });

    return () => {
      active = false;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [snapshot.todayCount]);

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
          detail="原图与缩略图"
          label="占用空间"
          value={formatBytes(snapshot.localStorageBytes)}
        />
        <MetricCard
          detail="客户端直连功能待接入"
          label="AI 任务"
          tone="amber"
          value={`${snapshot.pendingAiJobs} 项`}
        />
      </section>

      <section className="detail-grid">
        <article className="panel">
          <div className="panel__heading">
            <div>
              <h3>最近一张截图</h3>
            </div>
            <span>{recentCapture ? "已保存到本机" : "暂无记录"}</span>
          </div>
          {recentCapture ? (
            <figure className="recent-capture">
              <img
                alt={`${formatNextCapture(recentCapture.capture.capturedAtUtc)} 保存的最近截图`}
                onError={() => {
                  setRecentCapture(null);
                  setRecentError(true);
                }}
                src={recentCapture.url}
              />
              <figcaption>
                {formatNextCapture(recentCapture.capture.capturedAtUtc)}
                {" · "}
                {formatBytes(recentCapture.capture.fileSize)}
              </figcaption>
            </figure>
          ) : (
            <div className="empty-canvas">
              <span aria-hidden="true">{recentLoading ? "…" : "＋"}</span>
              <strong>
                {recentLoading
                  ? "正在读取最近截图"
                  : recentError
                    ? "最近截图暂时无法显示"
                    : "你的第一段旅程会出现在这里"}
              </strong>
              <p>
                {recentError
                  ? "可以切换到时间线重试，或重新打开今日页面。"
                  : "缩略图保存在本机，并会直接显示在时间线中。"}
              </p>
            </div>
          )}
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
                <strong>本地存储</strong>
                <p>普通 WebP · 不自动上传</p>
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
