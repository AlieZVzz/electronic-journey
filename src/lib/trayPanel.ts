import type {
  RecordingState,
  SuspensionReason,
  TraySnapshot,
} from "../types/app";

export type TrayTone = "active" | "paused" | "warning" | "neutral";

export interface TrayStatusPresentation {
  label: string;
  detail: string;
  tone: TrayTone;
}

const suspensionLabels: Record<SuspensionReason, string> = {
  screen_locked: "屏幕已锁定",
  system_sleeping: "系统正在休眠",
  user_idle: "用户暂时空闲",
};

export function trayStatusPresentation(
  snapshot: Pick<TraySnapshot, "state" | "suspensionReason">,
): TrayStatusPresentation {
  switch (snapshot.state) {
    case "running":
      return {
        label: "正在记录",
        detail: "截图只保存在这台设备上",
        tone: "active",
      };
    case "paused":
      return {
        label: "记录已暂停",
        detail: "恢复后才会继续安排截图",
        tone: "paused",
      };
    case "suspended":
      return {
        label: "系统暂挂",
        detail: snapshot.suspensionReason
          ? suspensionLabels[snapshot.suspensionReason]
          : "等待系统恢复后继续",
        tone: "warning",
      };
    case "degraded":
      return {
        label: "记录异常",
        detail: "请打开主窗口查看详情",
        tone: "warning",
      };
    default:
      return {
        label: "记录已停止",
        detail: "已保存的截图不会被删除",
        tone: "neutral",
      };
  }
}

export function trayActionAvailability(state: RecordingState) {
  const recordingActive = state === "running" || state === "suspended";
  return {
    start: !recordingActive,
    pause: recordingActive,
    stop: state !== "stopped",
  };
}
