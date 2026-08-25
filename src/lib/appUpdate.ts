import type { AppUpdateProgress } from "../types/app";

function formatBytes(value: number): string {
  if (value < 1024 * 1024) {
    return `${Math.max(1, Math.round(value / 1024))} KB`;
  }
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function appUpdateProgressText(
  progress: AppUpdateProgress | null,
): string {
  if (!progress || progress.phase === "installing") {
    return "正在验证更新签名并安装；应用随后会重新启动。";
  }
  if (!progress.totalBytes) {
    return `已下载 ${formatBytes(progress.downloadedBytes)}`;
  }
  return `已下载 ${formatBytes(progress.downloadedBytes)} / ${formatBytes(progress.totalBytes)}`;
}

export function appUpdateProgressPercent(
  progress: AppUpdateProgress | null,
): number | null {
  if (!progress?.totalBytes) {
    return null;
  }
  return Math.min(
    100,
    Math.max(0, (progress.downloadedBytes / progress.totalBytes) * 100),
  );
}
