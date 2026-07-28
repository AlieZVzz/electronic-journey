import type { CaptureSettings } from "../types/app";

export const captureIntervals = [1, 2, 5, 10, 15, 30, 60] as const;

export const defaultCaptureSettings: CaptureSettings = {
  intervalMinutes: 5,
  idlePauseMinutes: 10,
  webpQuality: 85,
  maxWidth: 2560,
  skipDuplicates: true,
};

export function validateCaptureSettings(settings: CaptureSettings): string[] {
  const errors: string[] = [];

  if (!captureIntervals.includes(settings.intervalMinutes as never)) {
    errors.push("截图间隔必须是受支持的预设值");
  }

  if (settings.idlePauseMinutes < 0 || settings.idlePauseMinutes > 240) {
    errors.push("空闲暂停时间必须介于 0 到 240 分钟");
  }

  if (settings.webpQuality < 1 || settings.webpQuality > 100) {
    errors.push("WebP 质量必须介于 1 到 100");
  }

  if (settings.maxWidth < 640 || settings.maxWidth > 7680) {
    errors.push("最大宽度必须介于 640 到 7680 像素");
  }

  return errors;
}
