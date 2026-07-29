import type { CaptureSettings, RecordingState } from "../types/app";

export type RuntimeAction = "permission" | "recording" | "settings";

export function recordingActionLabel(
  currentState: RecordingState,
  pendingState: RecordingState | null,
): string {
  if (pendingState === "running") {
    return "正在开始…";
  }
  if (pendingState === "paused") {
    return "正在暂停…";
  }
  if (pendingState === "stopped") {
    return "正在停止…";
  }
  return currentState === "running" ? "暂停记录" : "开始记录";
}

export function recordingSuccessMessage(state: RecordingState): string {
  switch (state) {
    case "running":
      return "记录已开始；首次截图将在 10 秒后执行。";
    case "paused":
      return "记录已暂停；不会再安排新的截图。";
    case "stopped":
      return "记录已停止；已保存的本地截图不会被删除。";
    default:
      return "记录状态已更新。";
  }
}

export function captureSettingsEqual(
  left: CaptureSettings,
  right: CaptureSettings,
): boolean {
  return (
    left.intervalMinutes === right.intervalMinutes &&
    left.idlePauseMinutes === right.idlePauseMinutes
  );
}
