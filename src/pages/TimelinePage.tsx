import {
  type MouseEvent as ReactMouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { desktopApi } from "../api/desktop";
import { groupTimelineCaptures } from "../lib/timeline";
import type { TimelineCapture } from "../types/app";

const MAX_PREVIEW_CACHE_ITEMS = 3;
const MAX_PREVIEW_CACHE_BYTES = 48 * 1024 * 1024;

type PreviewCacheEntry = {
  url: string;
  size: number;
};

type PreviewState = {
  capture: TimelineCapture;
  url: string | null;
  decoded: boolean;
  error: string | null;
  naturalWidth: number | null;
  naturalHeight: number | null;
};

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

function TimelineThumbnail({ captureId }: { captureId: string }) {
  const containerRef = useRef<HTMLSpanElement | null>(null);
  const [visible, setVisible] = useState(false);
  const [url, setUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    const element = containerRef.current;
    if (!element || visible) {
      return;
    }
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true);
          observer.disconnect();
        }
      },
      { rootMargin: "240px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [visible]);

  useEffect(() => {
    if (!visible) {
      return;
    }
    let active = true;
    let objectUrl: string | null = null;
    void desktopApi
      .readTimelineThumbnail(captureId)
      .then((bytes) => {
        if (!active) {
          return;
        }
        objectUrl = URL.createObjectURL(
          new Blob([bytes], { type: "image/webp" }),
        );
        setUrl(objectUrl);
      })
      .catch(() => {
        if (active) {
          setFailed(true);
        }
      });
    return () => {
      active = false;
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl);
      }
    };
  }, [captureId, visible]);

  return (
    <span className="timeline-card__canvas" ref={containerRef}>
      {url ? (
        <img alt="" src={url} />
      ) : (
        <>
          <i />
          <em>{failed ? "缩略图不可用" : "正在载入…"}</em>
        </>
      )}
    </span>
  );
}

export function TimelinePage() {
  const [captures, setCaptures] = useState<TimelineCapture[]>([]);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    capture: TimelineCapture;
    x: number;
    y: number;
  } | null>(null);
  const [deleteCandidate, setDeleteCandidate] =
    useState<TimelineCapture | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [previewScale, setPreviewScale] = useState<
    "fit" | "pixel" | number
  >("pixel");
  const previewCacheRef = useRef(new Map<string, PreviewCacheEntry>());
  const pendingPreviewsRef = useRef(new Map<string, Promise<string>>());
  const activePreviewIdRef = useRef<string | null>(null);
  const previewRequestRef = useRef(0);
  const deletedCaptureIdsRef = useRef(new Set<string>());
  const contextMenuRef = useRef<HTMLDivElement | null>(null);
  const deleteCancelButtonRef = useRef<HTMLButtonElement | null>(null);
  const groups = useMemo(() => groupTimelineCaptures(captures), [captures]);
  const resolvedPreviewScale =
    previewScale === "pixel"
      ? 1 / Math.max(1, window.devicePixelRatio)
      : previewScale;

  const trimPreviewCache = useCallback(() => {
    const cache = previewCacheRef.current;
    const cacheSize = () =>
      Array.from(cache.values()).reduce((total, entry) => total + entry.size, 0);
    while (
      cache.size > MAX_PREVIEW_CACHE_ITEMS ||
      cacheSize() > MAX_PREVIEW_CACHE_BYTES
    ) {
      const removableId = Array.from(cache.keys()).find(
        (id) => id !== activePreviewIdRef.current,
      );
      if (!removableId) {
        break;
      }
      const entry = cache.get(removableId);
      if (entry) {
        URL.revokeObjectURL(entry.url);
      }
      cache.delete(removableId);
    }
  }, []);

  const loadPreviewUrl = useCallback(
    (captureId: string): Promise<string> => {
      const cached = previewCacheRef.current.get(captureId);
      if (cached) {
        previewCacheRef.current.delete(captureId);
        previewCacheRef.current.set(captureId, cached);
        return Promise.resolve(cached.url);
      }
      const pending = pendingPreviewsRef.current.get(captureId);
      if (pending) {
        return pending;
      }
      const request = desktopApi
        .readTimelineCapture(captureId)
        .then((bytes) => {
          if (deletedCaptureIdsRef.current.has(captureId)) {
            throw new Error("截图已经被删除。");
          }
          const entry = {
            url: URL.createObjectURL(
              new Blob([bytes], { type: "image/webp" }),
            ),
            size: bytes.byteLength,
          };
          previewCacheRef.current.set(captureId, entry);
          trimPreviewCache();
          return entry.url;
        })
        .finally(() => {
          pendingPreviewsRef.current.delete(captureId);
        });
      pendingPreviewsRef.current.set(captureId, request);
      return request;
    },
    [trimPreviewCache],
  );

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
      previewRequestRef.current += 1;
      for (const entry of previewCacheRef.current.values()) {
        URL.revokeObjectURL(entry.url);
      }
      previewCacheRef.current.clear();
    },
    [],
  );

  useEffect(() => {
    if (!preview) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        previewRequestRef.current += 1;
        activePreviewIdRef.current = null;
        setPreview(null);
        trimPreviewCache();
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [preview, trimPreviewCache]);

  useEffect(() => {
    if (!contextMenu) {
      return;
    }
    contextMenuRef.current
      ?.querySelector<HTMLButtonElement>("button")
      ?.focus();
    const closeMenu = () => setContextMenu(null);
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        closeMenu();
      }
    };
    window.addEventListener("pointerdown", closeMenu);
    window.addEventListener("blur", closeMenu);
    window.addEventListener("resize", closeMenu);
    window.addEventListener("keydown", closeOnEscape);
    document.addEventListener("scroll", closeMenu, true);
    return () => {
      window.removeEventListener("pointerdown", closeMenu);
      window.removeEventListener("blur", closeMenu);
      window.removeEventListener("resize", closeMenu);
      window.removeEventListener("keydown", closeOnEscape);
      document.removeEventListener("scroll", closeMenu, true);
    };
  }, [contextMenu]);

  useEffect(() => {
    if (!deleteCandidate) {
      return;
    }
    deleteCancelButtonRef.current?.focus();
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && deletingId === null) {
        setDeleteCandidate(null);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [deleteCandidate, deletingId]);

  function closePreview() {
    previewRequestRef.current += 1;
    activePreviewIdRef.current = null;
    setPreview(null);
    trimPreviewCache();
  }

  async function openPreview(capture: TimelineCapture) {
    const requestId = previewRequestRef.current + 1;
    previewRequestRef.current = requestId;
    activePreviewIdRef.current = capture.id;
    setPreviewScale("pixel");
    setPreview({
      capture,
      url: null,
      decoded: false,
      error: null,
      naturalWidth: null,
      naturalHeight: null,
    });
    setError(null);
    try {
      const url = await loadPreviewUrl(capture.id);
      if (previewRequestRef.current !== requestId) {
        return;
      }
      setPreview((current) =>
        current?.capture.id === capture.id ? { ...current, url } : current,
      );
    } catch (reason) {
      if (previewRequestRef.current === requestId) {
        setPreview((current) =>
          current?.capture.id === capture.id
            ? { ...current, error: String(reason) }
            : current,
        );
      }
    }
  }

  function zoomPreview(delta: number) {
    setPreviewScale((current) => {
      const numeric =
        current === "fit"
          ? 1
          : current === "pixel"
            ? 1 / Math.max(1, window.devicePixelRatio)
            : current;
      return Math.min(2, Math.max(0.25, numeric + delta));
    });
  }

  function openContextMenu(
    event: ReactMouseEvent,
    capture: TimelineCapture,
  ) {
    event.preventDefault();
    openContextMenuAt(event.clientX, event.clientY, capture);
  }

  function openContextMenuAt(
    x: number,
    y: number,
    capture: TimelineCapture,
  ) {
    setContextMenu({
      capture,
      x: Math.max(8, Math.min(x, window.innerWidth - 184)),
      y: Math.max(8, Math.min(y, window.innerHeight - 112)),
    });
  }

  function evictPreview(captureId: string) {
    const cached = previewCacheRef.current.get(captureId);
    if (cached) {
      URL.revokeObjectURL(cached.url);
      previewCacheRef.current.delete(captureId);
    }
  }

  async function confirmDeleteCapture() {
    if (!deleteCandidate || deletingId !== null) {
      return;
    }
    const capture = deleteCandidate;
    setDeletingId(capture.id);
    setError(null);
    try {
      await desktopApi.deleteTimelineCapture(capture.id);
      deletedCaptureIdsRef.current.add(capture.id);
      evictPreview(capture.id);
      if (preview?.capture.id === capture.id) {
        closePreview();
      }
      setCaptures((current) =>
        current.filter(({ id }) => id !== capture.id),
      );
      setNextOffset((current) =>
        current === null ? null : Math.max(0, current - 1),
      );
      setDeleteCandidate(null);
      window.dispatchEvent(
        new Event("electronic-journey:snapshot-changed"),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDeletingId(null);
    }
  }

  return (
    <section className="timeline-page">
      <header className="page-header timeline-header">
        <div>
          <p className="eyebrow">LOCAL · VISUAL TIMELINE</p>
          <h1>时间线</h1>
          <p>按当前系统时区排列；缩略图和原图都从本地读取。</p>
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
          正在读取本地图片索引…
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
                    key={capture.id}
                    onClick={() => void openPreview(capture)}
                    onContextMenu={(event) =>
                      openContextMenu(event, capture)
                    }
                    onKeyDown={(event) => {
                      if (
                        event.key === "ContextMenu" ||
                        (event.shiftKey && event.key === "F10")
                      ) {
                        event.preventDefault();
                        const bounds =
                          event.currentTarget.getBoundingClientRect();
                        openContextMenuAt(
                          bounds.left + 18,
                          bounds.top + 18,
                          capture,
                        );
                      }
                    }}
                    type="button"
                  >
                    <TimelineThumbnail captureId={capture.id} />
                    <span className="timeline-card__meta">
                      <strong>{formatTime(capture.capturedAtUtc)}</strong>
                      <small>
                        {formatBytes(capture.fileSize)} · 本地图片
                      </small>
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

      {contextMenu && (
        <div
          aria-label="截图操作"
          className="timeline-context-menu"
          onPointerDown={(event) => event.stopPropagation()}
          ref={contextMenuRef}
          role="menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            onClick={() => {
              const { capture } = contextMenu;
              setContextMenu(null);
              void openPreview(capture);
            }}
            role="menuitem"
            type="button"
          >
            查看原图
          </button>
          <button
            className="is-danger"
            onClick={() => {
              setDeleteCandidate(contextMenu.capture);
              setContextMenu(null);
            }}
            role="menuitem"
            type="button"
          >
            删除图片…
          </button>
        </div>
      )}

      {deleteCandidate && (
        <div
          className="delete-dialog-backdrop"
          onMouseDown={(event) => {
            if (
              event.target === event.currentTarget &&
              deletingId === null
            ) {
              setDeleteCandidate(null);
            }
          }}
        >
          <section
            aria-labelledby="delete-dialog-title"
            aria-modal="true"
            className="delete-dialog"
            role="alertdialog"
          >
            <p className="eyebrow">PERMANENT LOCAL DELETE</p>
            <h2 id="delete-dialog-title">删除这张截图？</h2>
            <p>
              将永久删除{" "}
              <strong>{formatTime(deleteCandidate.capturedAtUtc)}</strong>{" "}
              保存的本地原图和缩略图。此操作无法在应用内撤销。
            </p>
            <div className="delete-dialog__actions">
              <button
                className="button button--ghost"
                disabled={deletingId !== null}
                onClick={() => setDeleteCandidate(null)}
                ref={deleteCancelButtonRef}
                type="button"
              >
                取消
              </button>
              <button
                className="button button--danger"
                disabled={deletingId !== null}
                onClick={() => void confirmDeleteCapture()}
                type="button"
              >
                {deletingId ? "正在删除并验证…" : "永久删除"}
              </button>
            </div>
          </section>
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
                <p className="eyebrow">LOCAL ORIGINAL</p>
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
            <div
              aria-busy={!preview.decoded && !preview.error}
              className={`preview-dialog__image ${
                previewScale === "fit"
                  ? "preview-dialog__image--fit"
                  : "preview-dialog__image--actual"
              }`}
            >
              {!preview.url && !preview.error && (
                <div className="preview-dialog__loading" role="status">
                  <i />
                  <span>正在读取本地原图…</span>
                </div>
              )}
              {preview.url && (
                <img
                  alt={`${formatTime(preview.capture.capturedAtUtc)} 保存的屏幕截图`}
                  className={preview.decoded ? "" : "is-loading"}
                  onError={() =>
                    setPreview((current) =>
                      current?.capture.id === preview.capture.id
                        ? {
                            ...current,
                            error: "图片解码失败，文件可能已经损坏。",
                          }
                        : current,
                    )
                  }
                  onLoad={(event) => {
                    const { naturalHeight, naturalWidth } = event.currentTarget;
                    setPreview((current) =>
                      current?.capture.id === preview.capture.id
                        ? {
                            ...current,
                            decoded: true,
                            naturalHeight,
                            naturalWidth,
                          }
                        : current,
                    );
                    const captureIndex = captures.findIndex(
                      ({ id }) => id === preview.capture.id,
                    );
                    const adjacent = [
                      captures[captureIndex - 1],
                      captures[captureIndex + 1],
                    ].filter(
                      (item): item is TimelineCapture => Boolean(item),
                    );
                    void Promise.allSettled(
                      adjacent.map((item) => loadPreviewUrl(item.id)),
                    );
                  }}
                  src={preview.url}
                  style={
                    resolvedPreviewScale === "fit" ||
                    preview.naturalWidth === null ||
                    preview.naturalHeight === null
                      ? undefined
                      : {
                          width:
                            preview.naturalWidth * resolvedPreviewScale,
                          height:
                            preview.naturalHeight * resolvedPreviewScale,
                        }
                  }
                />
              )}
              {preview.url && !preview.decoded && !preview.error && (
                <div className="preview-dialog__loading" role="status">
                  <i />
                  <span>正在解码原图…</span>
                </div>
              )}
              {preview.error && (
                <div className="preview-dialog__error" role="alert">
                  <strong>无法显示这张截图</strong>
                  <span>{preview.error}</span>
                  <button
                    className="button button--ghost"
                    onClick={() => void openPreview(preview.capture)}
                    type="button"
                  >
                    重试
                  </button>
                </div>
              )}
            </div>
            <footer className="preview-dialog__footer">
              <span>
                本地 WebP 原图
              </span>
              <div className="preview-dialog__controls">
                <button
                  aria-label="缩小截图"
                  disabled={!preview.decoded}
                  onClick={() => zoomPreview(-0.25)}
                  type="button"
                >
                  −
                </button>
                <button
                  className={previewScale === "fit" ? "is-active" : ""}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale("fit")}
                  type="button"
                >
                  适应窗口
                </button>
                <button
                  className={previewScale === "pixel" ? "is-active" : ""}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale("pixel")}
                  type="button"
                >
                  清晰 1:1
                </button>
                <button
                  className={previewScale === 1 ? "is-active" : ""}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale(1)}
                  type="button"
                >
                  源图 100%
                </button>
                <button
                  aria-label="放大截图"
                  disabled={!preview.decoded}
                  onClick={() => zoomPreview(0.25)}
                  type="button"
                >
                  +
                </button>
              </div>
              <span>
                {previewScale === "fit"
                  ? "适应窗口"
                  : previewScale === "pixel"
                    ? "像素级显示"
                  : `${Math.round(previewScale * 100)}%`}
                {" · "}
                {formatBytes(preview.capture.fileSize)}
              </span>
            </footer>
          </section>
        </div>
      )}
    </section>
  );
}
