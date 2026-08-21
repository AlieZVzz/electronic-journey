import type { RecordingState } from "../types/app";

const labels: Record<RecordingState, string> = {
  stopped: "记录已停止",
  running: "正在记录",
  paused: "已暂停",
  suspended: "系统暂挂",
  degraded: "需要处理",
};

export function StatusPill({ state }: { state: RecordingState }) {
  return (
    <span
      aria-label={`记录状态：${labels[state]}`}
      className={`status-pill status-pill--${state}`}
      role="status"
    >
      <i aria-hidden="true" />
      {labels[state]}
    </span>
  );
}
