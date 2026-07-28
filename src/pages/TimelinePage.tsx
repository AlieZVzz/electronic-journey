import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { desktopApi } from "../api/desktop";
import { groupTimelineCaptures } from "../lib/timeline";
import type { TimelineCapture } from "../types/app";

function formatTime(value: string): string {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(value));
}

function formatBytes(bytes: number): string {
  return bytes < 1024 * 1024
    ? `${Math.max(1, Math.round(bytes / 1024))} KB`
    : `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function TimelinePage() {
  const [captures, setCaptures] = useState<TimelineCapture[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingPreviewId, setLoadingPreviewId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<{
    capture: TimelineCapture;
    url: string;
  } | null>(null);
  const previewUrlRef = useRef<string | null>(null);
  const groups = useMemo(() => groupTimelineCaptures(captures), [captures]);

  const loadPage = useCallback(async (offset: number, replace: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const page = await desktopApi.listTimelineCaptures(offset);
      setCaptures((current) => {
        if (replace) {
          return page.items;
        }
        const known = new Set(current.map(({ id }) => id));
        return [
          ...current,
          ...page.items.filter(({ id }) => !known.has(id)),
        ];
      });
      setNextOffset(page.nextOffset);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadPage(0, true);
  }, [loadPage]);

  useEffect(
    () => () => {
      if (previewUrlRef.current) {
        URL.revokeObjectURL(previewUrlRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    if (!preview) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closePreview();
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [preview]);

  function closePreview() {
    if (previewUrlRef.current) {
      URL.revokeObjectURL(previewUrlRef.current);
      previewUrlRef.current = null;
    }
    setPreview(null);
  }

  async function openPreview(capture: TimelineCapture) {
    if (loadingPreviewId !== null) {
      return;
    }
    setLoadingPreviewId(capture.id);
    setError(null);
    try {
      const bytes = await desktopApi.readTimelineCapture(capture.id);
      if (previewUrlRef.current) {
        URL.revokeObjectURL(previewUrlRef.current);
      }
      const url = URL.createObjectURL(
        new Blob([bytes], { type: "image/webp" }),
      );
      previewUrlRef.current = url;
      setPreview({ capture, url });
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoadingPreviewId(null);
    }
  }

  return (
    <section className="timeline-page">
      <header className="page-header timeline-header">
        <div>
          <p className="eyebrow">LOCAL · ON-DEMAND DECRYPTION</p>
          <h1>时间线</h1>
          <p>按当前系统时区排列；只有点击某一张时才会在本机解密。</p>
        </div>
        <button
          className="button button--ghost"
          disabled={loading}
          onClick={() => void loadPage(0, true)}
          type="button"
        >
          刷新
        </button>
      </header>

      {error && (
        <div className="error-banner" role="alert">
          {error}
        </div>
      )}

      {loading && captures.length === 0 ? (
        <div className="timeline-loading" aria-live="polite">
          正在读取本地保险箱索引…
        </div>
      ) : captures.length === 0 ? (
        <div className="empty-canvas timeline-empty">
          <span aria-hidden="true">≋</span>
          <strong>时间线尚无记录</strong>
          <p>开始记录并完成第一张截图后，它会出现在这里。</p>
        </div>
      ) : (
        <div className="timeline-groups" aria-busy={loading}>
          {groups.map((group) => (
            <section className="timeline-day" key={group.dateKey}>
              <div className="timeline-day__heading">
                <h2>{group.label}</h2>
                <span>{group.items.length} 张</span>
              </div>
              <div className="timeline-grid">
                {group.items.map((capture) => (
                  <button
                    aria-label={`查看 ${formatTime(capture.capturedAtUtc)} 的截图`}
                    className="timeline-card"
                    disabled={loadingPreviewId !== null}
                    key={capture.id}
                    onClick={() => void openPreview(capture)}
                    type="button"
                  >
                    <span className="timeline-card__canvas" aria-hidden="true">
                      <i />
                      <em>
                        {loadingPreviewId === capture.id
                          ? "正在本机解密…"
                          : "点击查看"}
                      </em>
                    </span>
                    <span className="timeline-card__meta">
                      <strong>{formatTime(capture.capturedAtUtc)}</strong>
                      <small>{formatBytes(capture.cipherSize)} · 已加密</small>
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ))}
        </div>
      )}

      {nextOffset !== null && (
        <div className="timeline-more">
          <button
            className="button button--ghost"
            disabled={loading}
            onClick={() => void loadPage(nextOffset, false)}
            type="button"
          >
            {loading ? "正在载入…" : "加载更早的记录"}
          </button>
        </div>
      )}

      {preview && (
        <div
          className="preview-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) {
              closePreview();
            }
          }}
        >
          <section
            aria-labelledby="preview-title"
            aria-modal="true"
            className="preview-dialog"
            role="dialog"
          >
            <header className="preview-dialog__header">
              <div>
                <p className="eyebrow">DECRYPTED LOCALLY</p>
                <h2 id="preview-title">
                  {formatTime(preview.capture.capturedAtUtc)}
                </h2>
              </div>
              <button
                aria-label="关闭截图预览"
                className="preview-dialog__close"
                onClick={closePreview}
                type="button"
              >
                ×
              </button>
            </header>
            <div className="preview-dialog__image">
              <img
                alt={`${formatTime(preview.capture.capturedAtUtc)} 保存的屏幕截图`}
                src={preview.url}
              />
            </div>
            <footer className="preview-dialog__footer">
              <span>仅在本机内存中解密</span>
              <span>{formatBytes(preview.capture.cipherSize)} 密文</span>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}
