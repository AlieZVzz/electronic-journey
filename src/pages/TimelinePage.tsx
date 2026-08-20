import {
  type MouseEvent as ReactMouseEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { desktopApi } from "../api/desktop";
import {
  addTimelineSelection,
  groupTimelineCaptures,
} from "../lib/timeline";
import {
  activeUploadProgressMessage,
  interruptedUploadProgressMessage,
  uploadDiagnosticsSummary,
} from "../lib/uploadProgress";
import type {
  TimelineCapture,
  TimelineTag,
  UploadBatchProgress,
} from "../types/app";

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

type UploadConfirmation = {
  captureIds: string[];
  totalBytes: number;
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

function uploadStateLabel(state: TimelineCapture["uploadState"]): string | null {
  switch (state) {
    case "pending":
      return "等待上传";
    case "uploading":
      return "上传中";
    case "uploaded":
      return "已上传";
    case "failed":
      return "上传失败";
    case "cancelled":
      return "已取消上传";
    default:
      return null;
  }
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
  const [tags, setTags] = useState<TimelineTag[]>([]);
  const [favoriteOnly, setFavoriteOnly] = useState(false);
  const [tagFilter, setTagFilter] = useState<string | null>(null);
  const [nextOffset, setNextOffset] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingAction, setLoadingAction] = useState<
    "initial" | "refresh" | "more" | null
  >("initial");
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const [preview, setPreview] = useState<PreviewState | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    capture: TimelineCapture;
    x: number;
    y: number;
  } | null>(null);
  const [deleteCandidate, setDeleteCandidate] =
    useState<TimelineCapture | null>(null);
  const [deletingId, setDeletingId] = useState<string | null>(null);
  const [bulkDeleteCandidates, setBulkDeleteCandidates] = useState<
    TimelineCapture[] | null
  >(null);
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [selectedItems, setSelectedItems] = useState<Map<string, number>>(
    () => new Map(),
  );
  const [selectingDay, setSelectingDay] = useState<string | null>(null);
  const [uploadProgress, setUploadProgress] =
    useState<UploadBatchProgress | null>(null);
  const [uploadMessage, setUploadMessage] = useState<string | null>(null);
  const [uploadConfirmation, setUploadConfirmation] =
    useState<UploadConfirmation | null>(null);
  const [startingUpload, setStartingUpload] = useState(false);
  const [uploadAction, setUploadAction] = useState<
    "retrying" | "cancelling" | null
  >(null);
  const [previewScale, setPreviewScale] = useState<
    "fit" | "pixel" | number
  >("fit");
  const previewCacheRef = useRef(new Map<string, PreviewCacheEntry>());
  const pendingPreviewsRef = useRef(new Map<string, Promise<string>>());
  const activePreviewIdRef = useRef<string | null>(null);
  const previewRequestRef = useRef(0);
  const deletedCaptureIdsRef = useRef(new Set<string>());
  const contextMenuRef = useRef<HTMLDivElement | null>(null);
  const deleteCancelButtonRef = useRef<HTMLButtonElement | null>(null);
  const groups = useMemo(() => groupTimelineCaptures(captures), [captures]);
  const selectedIds = useMemo(
    () => new Set(selectedItems.keys()),
    [selectedItems],
  );
  const uploadActive =
    uploadProgress?.state === "pending" ||
    uploadProgress?.state === "uploading";
  const uploadDiagnostics = uploadProgress
    ? uploadDiagnosticsSummary(uploadProgress)
    : null;
  const resolvedPreviewScale =
    previewScale === "pixel"
      ? 1 / Math.max(1, window.devicePixelRatio)
      : previewScale;
  const numericPreviewScale =
    previewScale === "fit"
      ? null
      : previewScale === "pixel"
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

  const loadPage = useCallback(
    async (
      offset: number,
      replace: boolean,
      action: "initial" | "refresh" | "more",
    ) => {
      setLoading(true);
      setLoadingAction(action);
      setError(null);
      setActionMessage(null);
      try {
        const page = await desktopApi.listTimelineCaptures(
          offset,
          18,
          favoriteOnly,
          tagFilter,
        );
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
        if (action === "refresh") {
          setActionMessage(
            `时间线已刷新，当前显示 ${page.items.length} 张记录。`,
          );
        } else if (action === "more") {
          setActionMessage(
            page.items.length > 0
              ? `已加载 ${page.items.length} 张更早的记录。`
              : "没有更早的记录了。",
          );
        }
      } catch (reason) {
        setError(String(reason));
      } finally {
        setLoading(false);
        setLoadingAction(null);
      }
    },
    [favoriteOnly, tagFilter],
  );

  const applyUploadProgress = useCallback(
    (progress: UploadBatchProgress) => {
      const states = new Map(
        progress.items.map((item) => [item.captureId, item.state]),
      );
      setCaptures((current) =>
        current.map((capture) => {
          const uploadState = states.get(capture.id);
          return uploadState
            ? { ...capture, uploadState }
            : capture;
        }),
      );
    },
    [],
  );

  useEffect(() => {
    void loadPage(0, true, "initial");
  }, [loadPage]);

  useEffect(() => {
    void desktopApi
      .listTimelineTags()
      .then(setTags)
      .catch((reason) => setError(String(reason)));
  }, []);

  async function toggleFavorite(capture: TimelineCapture) {
    const favorite = !capture.favorite;
    setError(null);
    setCaptures((current) =>
      current.map((item) =>
        item.id === capture.id ? { ...item, favorite } : item,
      ),
    );
    try {
      await desktopApi.setTimelineCaptureFavorite(capture.id, favorite);
      if (favoriteOnly && !favorite) {
        setCaptures((current) =>
          current.filter((item) => item.id !== capture.id),
        );
      }
      setActionMessage(favorite ? "已加入收藏。" : "已取消收藏。");
    } catch (reason) {
      setCaptures((current) =>
        current.map((item) =>
          item.id === capture.id
            ? { ...item, favorite: capture.favorite }
            : item,
        ),
      );
      setError(String(reason));
    }
  }

  async function editCaptureTags(capture: TimelineCapture) {
    const input = window.prompt(
      "输入标签，用逗号分隔；清空可移除全部标签。",
      capture.tags.map((tag) => tag.name).join(", "),
    );
    if (input === null) {
      return;
    }
    const names = Array.from(
      new Map(
        input
          .split(/[,，]/)
          .map((name) => name.trim())
          .filter(Boolean)
          .map((name) => [name.toLocaleLowerCase(), name]),
      ).values(),
    );
    if (names.length > 20) {
      setError("每张截图最多添加 20 个标签。");
      return;
    }
    setError(null);
    try {
      const resolved = await Promise.all(
        names.map((name) => desktopApi.createTimelineTag(name)),
      );
      await desktopApi.setTimelineCaptureTags(
        capture.id,
        resolved.map((tag) => tag.id),
      );
      const allTags = await desktopApi.listTimelineTags();
      setTags(allTags);
      setCaptures((current) =>
        current.map((item) =>
          item.id === capture.id ? { ...item, tags: resolved } : item,
        ),
      );
      if (tagFilter && !resolved.some((tag) => tag.id === tagFilter)) {
        setCaptures((current) =>
          current.filter((item) => item.id !== capture.id),
        );
      }
      setActionMessage("截图标签已更新。");
    } catch (reason) {
      setError(String(reason));
    }
  }

  async function deleteSelectedTag() {
    if (!tagFilter) {
      return;
    }
    const tag = tags.find((item) => item.id === tagFilter);
    if (!tag || !window.confirm(`删除标签“${tag.name}”？截图本身不会被删除。`)) {
      return;
    }
    setError(null);
    try {
      await desktopApi.deleteTimelineTag(tag.id);
      setTags((current) => current.filter((item) => item.id !== tag.id));
      setCaptures((current) =>
        current.map((capture) => ({
          ...capture,
          tags: capture.tags.filter((item) => item.id !== tag.id),
        })),
      );
      setTagFilter(null);
      setActionMessage(`标签“${tag.name}”已删除；截图未受影响。`);
    } catch (reason) {
      setError(String(reason));
    }
  }

  useEffect(() => {
    let active = true;
    void desktopApi
      .getActiveUploadBatch()
      .then((progress) => {
        if (progress) {
          if (!active) {
            return null;
          }
          setUploadProgress(progress);
          applyUploadProgress(progress);
          setUploadMessage(activeUploadProgressMessage(progress));
          return null;
        }
        return desktopApi.getLatestUnhandledInterruptedUploadBatch();
      })
      .then((recoveredProgress) => {
        if (!active || !recoveredProgress) {
          return;
        }
        setUploadProgress(recoveredProgress);
        applyUploadProgress(recoveredProgress);
        setUploadMessage(
          interruptedUploadProgressMessage(recoveredProgress),
        );
      })
      .catch((reason) => {
        if (active) {
          setError(String(reason));
        }
      });
    return () => {
      active = false;
    };
  }, [applyUploadProgress]);

  useEffect(() => {
    if (uploadActive || !desktopApi.isDesktopRuntime()) {
      return;
    }
    let active = true;
    const timer = window.setInterval(() => {
      void desktopApi
        .getActiveUploadBatch()
        .then((progress) => {
          if (!active || !progress) {
            return;
          }
          setUploadProgress(progress);
          applyUploadProgress(progress);
          setUploadMessage(activeUploadProgressMessage(progress));
        })
        .catch((reason) => {
          if (active) {
            setError(String(reason));
          }
        });
    }, 5_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [applyUploadProgress, uploadActive]);

  useEffect(() => {
    if (!uploadActive || !uploadProgress) {
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      void desktopApi
        .getUploadBatchStatus(uploadProgress.batchId)
        .then((progress) => {
          if (!active) {
            return;
          }
          setUploadProgress(progress);
          applyUploadProgress(progress);
          if (
            progress.state === "pending" ||
            progress.state === "uploading"
          ) {
            setUploadMessage(activeUploadProgressMessage(progress));
            return;
          }
          setUploadMessage(
            progress.state === "cancelled"
              ? `后台上传已取消：${progress.uploadedItems} 张已完成，剩余项目未继续上传。`
              : progress.failedItems === 0
              ? `后台上传完成：已验证上传 ${progress.uploadedItems} 张原图。`
              : `后台上传完成：${progress.uploadedItems} 张成功，${progress.failedItems} 张失败。${
                  progress.lastError ? ` ${progress.lastError}` : ""
                }`,
          );
          window.dispatchEvent(
            new Event("electronic-journey:snapshot-changed"),
          );
        })
        .catch((reason) => {
          if (active) {
            setError(String(reason));
          }
        });
    }, 800);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [applyUploadProgress, uploadActive, uploadProgress]);

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

  useEffect(() => {
    if (!uploadConfirmation) {
      return;
    }
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setUploadConfirmation(null);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [uploadConfirmation]);

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
    setPreviewScale("fit");
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
      setSelectedItems((current) => {
        const next = new Map(current);
        next.delete(capture.id);
        return next;
      });
      setNextOffset((current) =>
        current === null ? null : Math.max(0, current - 1),
      );
      setDeleteCandidate(null);
      setActionMessage(
        `${formatTime(capture.capturedAtUtc)} 的本地截图已删除并完成验证。`,
      );
      window.dispatchEvent(
        new Event("electronic-journey:snapshot-changed"),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDeletingId(null);
    }
  }

  function prepareBulkDelete() {
    if (selectedIds.size === 0 || bulkDeleting) {
      return;
    }
    const candidates = captures.filter((capture) =>
      selectedIds.has(capture.id),
    );
    if (candidates.length === 0) {
      setError("所选截图已不在当前时间线，请刷新后重试。");
      return;
    }
    if (candidates.length !== selectedIds.size) {
      setError("删除所选目前只支持已加载到页面的截图，请先加载更多记录。");
      return;
    }
    setBulkDeleteCandidates(candidates);
  }

  async function confirmBulkDelete() {
    if (!bulkDeleteCandidates || bulkDeleting) {
      return;
    }
    const candidates = bulkDeleteCandidates;
    const activeUploads = candidates.filter(
      (capture) =>
        capture.uploadState === "pending" ||
        capture.uploadState === "uploading",
    );
    if (activeUploads.length > 0) {
      setBulkDeleteCandidates(null);
      setError(
        `所选截图中有 ${activeUploads.length} 张正在上传，请等待上传结束后再删除。`,
      );
      return;
    }

    setBulkDeleting(true);
    setError(null);
    const deletedIds = new Set<string>();
    let failedCount = 0;
    try {
      for (const capture of candidates) {
        try {
          await desktopApi.deleteTimelineCapture(capture.id);
          deletedIds.add(capture.id);
          deletedCaptureIdsRef.current.add(capture.id);
          evictPreview(capture.id);
          if (preview?.capture.id === capture.id) {
            closePreview();
          }
        } catch {
          failedCount += 1;
        }
      }
      if (deletedIds.size > 0) {
        setCaptures((current) =>
          current.filter(({ id }) => !deletedIds.has(id)),
        );
        setSelectedItems((current) => {
          const next = new Map(current);
          for (const id of deletedIds) {
            next.delete(id);
          }
          return next;
        });
        setNextOffset((current) =>
          current === null
            ? null
            : Math.max(0, current - deletedIds.size),
        );
        window.dispatchEvent(
          new Event("electronic-journey:snapshot-changed"),
        );
      }
      setBulkDeleteCandidates(null);
      setActionMessage(
        failedCount === 0
          ? `已永久删除 ${deletedIds.size} 张本地截图并完成验证。`
          : `已删除 ${deletedIds.size} 张截图，${failedCount} 张删除失败，请刷新后重试。`,
      );
    } finally {
      setBulkDeleting(false);
    }
  }

  function toggleCaptureSelection(capture: TimelineCapture) {
    setSelectedItems((current) => {
      const next = new Map(current);
      if (next.has(capture.id)) {
        next.delete(capture.id);
      } else {
        next.set(capture.id, capture.fileSize);
      }
      return next;
    });
    setUploadConfirmation(null);
    setUploadMessage(null);
  }

  function toggleAllLoadedCaptures() {
    setSelectedItems((current) => {
      const allLoadedSelected =
        captures.length > 0 &&
        captures.every((capture) => current.has(capture.id));
      if (allLoadedSelected) {
        return new Map();
      }
      const next = new Map(current);
      for (const capture of captures) {
        next.set(capture.id, capture.fileSize);
      }
      return next;
    });
    setUploadConfirmation(null);
    setUploadMessage(null);
  }

  async function selectEntireDay(dateKey: string) {
    if (selectingDay !== null) {
      return;
    }
    setSelectingDay(dateKey);
    setError(null);
    setActionMessage(null);
    setUploadConfirmation(null);
    setUploadMessage(null);
    try {
      const dayItems =
        await desktopApi.listTimelineDaySelection(dateKey);
      setSelectedItems((current) =>
        addTimelineSelection(current, dayItems),
      );
      setActionMessage(`已选择当天全部 ${dayItems.length} 张截图。`);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSelectingDay(null);
    }
  }

  function prepareUploadSelection() {
    if (selectedIds.size === 0 || uploadActive || startingUpload) {
      return;
    }
    if (selectedItems.size > 500) {
      setError("单次最多上传 500 张截图，请取消部分选择后重试。");
      return;
    }
    setUploadConfirmation({
      captureIds: Array.from(selectedItems.keys()),
      totalBytes: Array.from(selectedItems.values()).reduce(
        (total, fileSize) => total + fileSize,
        0,
      ),
    });
  }

  async function startConfirmedUpload() {
    if (!uploadConfirmation || uploadActive || startingUpload) {
      return;
    }
    const selection = uploadConfirmation;
    setUploadConfirmation(null);
    setStartingUpload(true);
    setError(null);
    setUploadMessage(null);
    try {
      await new Promise<void>((resolve) => {
        window.requestAnimationFrame(() => resolve());
      });
      const progress = await desktopApi.uploadSelectedCaptures(
        selection.captureIds,
      );
      setUploadProgress(progress);
      applyUploadProgress(progress);
      setUploadMessage(activeUploadProgressMessage(progress));
      setSelectedItems(new Map());
      window.dispatchEvent(
        new Event("electronic-journey:snapshot-changed"),
      );
    } catch (reason) {
      await loadPage(0, true, "refresh");
      setError(String(reason));
    } finally {
      setStartingUpload(false);
    }
  }

  async function retryFailedUploadItems() {
    if (
      !uploadProgress ||
      uploadActive ||
      uploadProgress.failedItems === 0 ||
      uploadAction !== null
    ) {
      return;
    }
    setUploadAction("retrying");
    setError(null);
    try {
      const progress = await desktopApi.retryFailedUploadItems(
        uploadProgress.batchId,
      );
      setUploadProgress(progress);
      applyUploadProgress(progress);
      setUploadMessage(activeUploadProgressMessage(progress));
      window.dispatchEvent(
        new Event("electronic-journey:snapshot-changed"),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setUploadAction(null);
    }
  }

  async function cancelActiveUpload() {
    if (!uploadProgress || !uploadActive || uploadAction !== null) {
      return;
    }
    if (
      !window.confirm(
        "取消后台上传？尚未开始的项目会停止，当前正在传输的项目可能完成。",
      )
    ) {
      return;
    }
    setUploadAction("cancelling");
    setError(null);
    try {
      const progress = await desktopApi.cancelUploadBatch(
        uploadProgress.batchId,
      );
      setUploadProgress(progress);
      applyUploadProgress(progress);
      setUploadMessage(
        `后台上传已取消：${progress.uploadedItems} 张已完成，剩余项目未继续上传。`,
      );
      window.dispatchEvent(
        new Event("electronic-journey:snapshot-changed"),
      );
    } catch (reason) {
      setError(String(reason));
    } finally {
      setUploadAction(null);
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
        <div className="timeline-header__actions">
          <label className="timeline-filter-toggle">
            <input
              checked={favoriteOnly}
              onChange={(event) => setFavoriteOnly(event.currentTarget.checked)}
              type="checkbox"
            />
            仅看收藏
          </label>
          <select
            aria-label="按标签筛选"
            onChange={(event) => setTagFilter(event.currentTarget.value || null)}
            value={tagFilter ?? ""}
          >
            <option value="">全部标签</option>
            {tags.map((tag) => (
              <option key={tag.id} value={tag.id}>{tag.name}</option>
            ))}
          </select>
          {tagFilter && (
            <button
              className="button button--ghost"
              onClick={() => void deleteSelectedTag()}
              type="button"
            >
              删除当前标签
            </button>
          )}
          <span aria-live="polite">
            {selectedIds.size > 0
              ? `已选 ${selectedIds.size} 张`
              : "尚未选择"}
          </span>
          <button
            className="button button--ghost"
            disabled={captures.length === 0}
            onClick={toggleAllLoadedCaptures}
            type="button"
          >
            {captures.length > 0 &&
            captures.every((capture) => selectedIds.has(capture.id))
              ? "取消全选"
              : "全选已加载"}
          </button>
          <button
            className="button button--primary"
            aria-busy={startingUpload}
            disabled={
              selectedIds.size === 0 || uploadActive || startingUpload
            }
            onClick={prepareUploadSelection}
            type="button"
          >
            {startingUpload
              ? "正在建立后台任务…"
              : uploadActive
              ? `后台上传 ${
                  (uploadProgress?.uploadedItems ?? 0) +
                  (uploadProgress?.failedItems ?? 0)
                } / ${uploadProgress?.totalItems ?? 0}`
              : "上传所选"}
          </button>
          <button
            className="button button--danger"
            disabled={selectedIds.size === 0 || bulkDeleting}
            onClick={prepareBulkDelete}
            type="button"
          >
            {bulkDeleting ? "正在删除…" : "删除所选"}
          </button>
          <button
            className="button button--ghost"
            aria-busy={loadingAction === "refresh"}
            disabled={loading}
            onClick={() => void loadPage(0, true, "refresh")}
            type="button"
          >
            {loadingAction === "refresh" ? "正在刷新…" : "刷新"}
          </button>
        </div>
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
          <strong>{favoriteOnly || tagFilter ? "没有符合筛选的截图" : "时间线尚无记录"}</strong>
          <p>{favoriteOnly || tagFilter ? "调整收藏或标签筛选后再试。" : "开始记录并完成第一张截图后，它会出现在这里。"}</p>
        </div>
      ) : (
        <div className="timeline-groups" aria-busy={loading}>
          {groups.map((group) => (
            <section className="timeline-day" key={group.dateKey}>
              <div className="timeline-day__heading">
                <h2>{group.label}</h2>
                <div className="timeline-day__actions">
                  <span>{group.items.length} 张已加载</span>
                  <button
                    aria-busy={selectingDay === group.dateKey}
                    className="timeline-day__select-all"
                    disabled={selectingDay !== null}
                    onClick={() =>
                      void selectEntireDay(group.dateKey)
                    }
                    type="button"
                  >
                    {selectingDay === group.dateKey
                      ? "正在选择…"
                      : "全选当天"}
                  </button>
                </div>
              </div>
              <div className="timeline-grid">
                {group.items.map((capture) => {
                  const stateLabel = uploadStateLabel(
                    capture.uploadState,
                  );
                  return (
                    <div
                      className={`timeline-card-wrap ${
                        selectedIds.has(capture.id) ? "is-selected" : ""
                      }`}
                      key={capture.id}
                    >
                      <label
                        className="timeline-card__select"
                        title="选择截图用于上传或删除"
                      >
                        <input
                          aria-label={`选择 ${formatTime(
                            capture.capturedAtUtc,
                          )} 的截图`}
                          checked={selectedIds.has(capture.id)}
                          onChange={() =>
                            toggleCaptureSelection(capture)
                          }
                          type="checkbox"
                        />
                        <span aria-hidden="true" />
                      </label>
                      {stateLabel && (
                        <span
                          className={`timeline-card__upload-state is-${capture.uploadState}`}
                        >
                          {stateLabel}
                        </span>
                      )}
                      <button
                        aria-label={capture.favorite ? "取消收藏" : "加入收藏"}
                        className={`timeline-card__favorite${capture.favorite ? " is-active" : ""}`}
                        onClick={() => void toggleFavorite(capture)}
                        title={capture.favorite ? "取消收藏" : "加入收藏"}
                        type="button"
                      >
                        {capture.favorite ? "★" : "☆"}
                      </button>
                      <button
                        aria-label={`查看 ${formatTime(capture.capturedAtUtc)} 的截图`}
                        className="timeline-card"
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
                          <strong>
                            {formatTime(capture.capturedAtUtc)}
                          </strong>
                          <small>
                            {formatBytes(capture.fileSize)} · 本地图片
                          </small>
                          {capture.tags.length > 0 && (
                            <span className="timeline-card__tags">
                              {capture.tags.slice(0, 3).map((tag) => (
                                <em key={tag.id}>{tag.name}</em>
                              ))}
                              {capture.tags.length > 3 && <em>+{capture.tags.length - 3}</em>}
                            </span>
                          )}
                        </span>
                      </button>
                    </div>
                  );
                })}
              </div>
            </section>
          ))}
        </div>
      )}

      {nextOffset !== null && (
        <div className="timeline-more">
          <button
            className="button button--ghost"
            aria-busy={loadingAction === "more"}
            disabled={loading}
            onClick={() => void loadPage(nextOffset, false, "more")}
            type="button"
          >
            {loading ? "正在载入…" : "加载更早的记录"}
          </button>
        </div>
      )}

      {(uploadConfirmation || uploadMessage || actionMessage) && (
        <aside
          aria-label="操作提示"
          className="upload-toast-region"
        >
          {uploadConfirmation && (
            <div
              aria-labelledby="upload-confirmation-title"
              className="upload-confirmation-toast"
              role="dialog"
            >
              <strong id="upload-confirmation-title">上传所选原图？</strong>
              <p>
                将 {uploadConfirmation.captureIds.length} 张原图（
                {formatBytes(uploadConfirmation.totalBytes)}
                ）上传到已配置的个人服务器文件夹。
              </p>
              <div className="upload-confirmation-toast__actions">
                <button
                  className="button button--ghost"
                  onClick={() => setUploadConfirmation(null)}
                  type="button"
                >
                  取消
                </button>
                <button
                  className="button button--primary"
                  onClick={() => void startConfirmedUpload()}
                  type="button"
                >
                  确认上传
                </button>
              </div>
            </div>
          )}
          {uploadMessage && (
            <div
              className={`upload-progress-toast ${
                !uploadActive && (uploadProgress?.failedItems ?? 0) > 0
                  ? "is-warning"
                  : ""
              }`}
              role="status"
            >
              <span>
                <strong>{uploadMessage}</strong>
                {uploadDiagnostics && <small>{uploadDiagnostics}</small>}
              </span>
              {uploadActive && (
                <button
                  className="upload-progress-toast__action"
                  disabled={uploadAction !== null}
                  onClick={() => void cancelActiveUpload()}
                  type="button"
                >
                  {uploadAction === "cancelling" ? "正在取消…" : "取消上传"}
                </button>
              )}
              {!uploadActive && (uploadProgress?.failedItems ?? 0) > 0 && (
                <button
                  className="upload-progress-toast__action"
                  disabled={uploadAction !== null}
                  onClick={() => void retryFailedUploadItems()}
                  type="button"
                >
                  {uploadAction === "retrying"
                    ? "正在重试…"
                    : `重试失败项（${uploadProgress?.failedItems ?? 0}）`}
                </button>
              )}
              {!uploadActive && (
                <button
                  aria-label="关闭上传提示"
                  onClick={() => setUploadMessage(null)}
                  type="button"
                >
                  ×
                </button>
              )}
            </div>
          )}
          {actionMessage && (
            <div className="upload-progress-toast" role="status">
              <span>{actionMessage}</span>
              <button
                aria-label="关闭操作提示"
                onClick={() => setActionMessage(null)}
                type="button"
              >
                ×
              </button>
            </div>
          )}
        </aside>
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
            onClick={() => {
              toggleCaptureSelection(contextMenu.capture);
              setContextMenu(null);
            }}
            role="menuitem"
            type="button"
          >
            {selectedIds.has(contextMenu.capture.id)
              ? "取消选择"
              : "选择用于上传"}
          </button>
          <button
            onClick={() => {
              const { capture } = contextMenu;
              setContextMenu(null);
              void toggleFavorite(capture);
            }}
            role="menuitem"
            type="button"
          >
            {contextMenu.capture.favorite ? "取消收藏" : "加入收藏"}
          </button>
          <button
            onClick={() => {
              const { capture } = contextMenu;
              setContextMenu(null);
              void editCaptureTags(capture);
            }}
            role="menuitem"
            type="button"
          >
            编辑标签…
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

      {bulkDeleteCandidates && (
        <div
          className="delete-dialog-backdrop"
          onMouseDown={(event) => {
            if (
              event.target === event.currentTarget &&
              !bulkDeleting
            ) {
              setBulkDeleteCandidates(null);
            }
          }}
        >
          <section
            aria-labelledby="bulk-delete-dialog-title"
            aria-modal="true"
            className="delete-dialog"
            role="alertdialog"
          >
            <p className="eyebrow">PERMANENT LOCAL DELETE</p>
            <h2 id="bulk-delete-dialog-title">删除所选截图？</h2>
            <p>
              将永久删除所选的 <strong>{bulkDeleteCandidates.length} 张截图</strong> 的本地原图和缩略图。
              此操作无法在应用内撤销。
            </p>
            <div className="delete-dialog__actions">
              <button
                className="button button--ghost"
                disabled={bulkDeleting}
                onClick={() => setBulkDeleteCandidates(null)}
                type="button"
              >
                取消
              </button>
              <button
                className="button button--danger"
                disabled={bulkDeleting}
                onClick={() => void confirmBulkDelete()}
                type="button"
              >
                {bulkDeleting ? "正在删除并验证…" : "永久删除所选"}
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
                  disabled={
                    !preview.decoded ||
                    (numericPreviewScale !== null &&
                      numericPreviewScale <= 0.25)
                  }
                  onClick={() => zoomPreview(-0.25)}
                  type="button"
                >
                  −
                </button>
                <button
                  className={previewScale === "fit" ? "is-active" : ""}
                  aria-pressed={previewScale === "fit"}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale("fit")}
                  type="button"
                >
                  适应窗口
                </button>
                <button
                  className={previewScale === "pixel" ? "is-active" : ""}
                  aria-pressed={previewScale === "pixel"}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale("pixel")}
                  type="button"
                >
                  清晰 1:1
                </button>
                <button
                  className={previewScale === 1 ? "is-active" : ""}
                  aria-pressed={previewScale === 1}
                  disabled={!preview.decoded}
                  onClick={() => setPreviewScale(1)}
                  type="button"
                >
                  源图 100%
                </button>
                <button
                  aria-label="放大截图"
                  disabled={
                    !preview.decoded ||
                    (numericPreviewScale !== null && numericPreviewScale >= 2)
                  }
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
