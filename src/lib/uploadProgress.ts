import type {
  UploadBatchProgress,
  UploadPhase,
} from "../types/app";

const phaseLabels: Record<UploadPhase, string> = {
  pending: "准备上传",
  connecting: "正在连接服务器",
  authenticating: "正在验证身份",
  initializing_sftp: "正在初始化 SFTP",
  validating_local: "正在校验本地原图",
  preparing_remote: "正在准备远端目录",
  transferring: "正在传输",
  verifying_remote: "正在核对远端文件",
  completed: "上传完成",
  failed: "上传结束",
};

function formatMebibytes(bytes: number): string {
  return `${(Math.max(0, bytes) / 1024 / 1024).toFixed(1)} MiB`;
}

function formatDuration(seconds: number): string {
  if (seconds < 60) {
    return `${Math.max(1, Math.ceil(seconds))} 秒`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = Math.ceil(seconds % 60);
  return remainingSeconds === 0
    ? `${minutes} 分钟`
    : `${minutes} 分 ${remainingSeconds} 秒`;
}

function formatMilliseconds(milliseconds: number): string {
  return milliseconds < 1000
    ? `${milliseconds} ms`
    : `${(milliseconds / 1000).toFixed(1)} s`;
}

export function activeUploadProgressMessage(
  progress: UploadBatchProgress,
): string {
  const performance = progress.performance;
  const processed = progress.uploadedItems + progress.failedItems;
  const details = [
    `${formatMebibytes(performance.uploadedBytes)} / ${formatMebibytes(progress.totalBytes)}`,
  ];
  if (performance.bytesPerSecond > 0) {
    details.push(`${formatMebibytes(performance.bytesPerSecond)}/s`);
  }
  if (performance.estimatedRemainingSeconds !== null) {
    details.push(
      `预计剩余 ${formatDuration(performance.estimatedRemainingSeconds)}`,
    );
  }
  details.push(`${processed} / ${progress.totalItems} 张`);
  return `${phaseLabels[performance.phase]}：${details.join(" · ")}`;
}

export function interruptedUploadProgressMessage(
  progress: UploadBatchProgress,
): string {
  return `上次应用退出时中断了 ${progress.failedItems} 张截图的上传，已暂停等待处理；不会自动重传。`;
}

export function uploadDiagnosticsSummary(
  progress: UploadBatchProgress,
): string | null {
  const performance = progress.performance;
  const hasMeasurements =
    performance.connectionMs > 0 ||
    performance.authenticationMs > 0 ||
    performance.sftpInitializationMs > 0 ||
    performance.localValidationMs > 0 ||
    performance.transferMs > 0 ||
    performance.remoteMetadataOperations > 0;
  if (!hasMeasurements) {
    return null;
  }

  const parts = [
    `连接 ${formatMilliseconds(performance.connectionMs)}`,
    `认证 ${formatMilliseconds(performance.authenticationMs)}`,
    `SFTP ${formatMilliseconds(performance.sftpInitializationMs)}`,
    `本地校验 ${formatMilliseconds(performance.localValidationMs)}`,
  ];
  if (performance.transferMs > 0) {
    parts.push(
      `传输 ${formatMilliseconds(performance.transferMs)} / ${formatMebibytes(performance.bytesPerSecond)}/s`,
    );
  }
  if (performance.remoteMetadataOperations > 0) {
    parts.push(
      `远端状态 ${performance.remoteMetadataOperations} 次 / ${formatMilliseconds(performance.remoteMetadataMs)}`,
    );
  }
  return `本地诊断：${parts.join(" · ")}`;
}
