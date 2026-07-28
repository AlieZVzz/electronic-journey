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
    <span className={`status-pill status-pill--${state}`}>
      <i aria-hidden="true" />
      {labels[state]}
    </span>
  );
}
