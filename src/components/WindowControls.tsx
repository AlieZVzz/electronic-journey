import { getCurrentWindow } from "@tauri-apps/api/window";

function runWindowAction(action: () => Promise<void>) {
  void action().catch(() => {
    // Window operations are best-effort controls; keep the UI responsive if
    // the native window is already closing or otherwise unavailable.
  });
}

export function WindowControls() {
  return (
    <div className="window-controls" aria-label="窗口控制">
      <button
        aria-label="最小化"
        className="window-control"
        onClick={() =>
          runWindowAction(() => getCurrentWindow().minimize())
        }
        title="最小化"
        type="button"
      >
        <svg aria-hidden="true" viewBox="0 0 12 12">
          <path d="M2 8.5h8" />
        </svg>
      </button>
      <button
        aria-label="最大化或还原"
        className="window-control"
        onClick={() =>
          runWindowAction(() => getCurrentWindow().toggleMaximize())
        }
        title="最大化或还原"
        type="button"
      >
        <svg aria-hidden="true" viewBox="0 0 12 12">
          <rect height="7" width="7" x="2.5" y="2.5" />
        </svg>
      </button>
      <button
        aria-label="关闭"
        className="window-control window-control--close"
        onClick={() => runWindowAction(() => getCurrentWindow().close())}
        title="关闭"
        type="button"
      >
        <svg aria-hidden="true" viewBox="0 0 12 12">
          <path d="m2.5 2.5 7 7m0-7-7 7" />
        </svg>
      </button>
    </div>
  );
}
