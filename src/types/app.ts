export type RecordingState =
  | "stopped"
  | "running"
  | "paused"
  | "suspended"
  | "degraded";

export type PermissionState = "not_determined" | "granted" | "denied";

export interface CaptureSettings {
  intervalMinutes: number;
  idlePauseMinutes: number;
  webpQuality: number;
  maxWidth: number;
  skipDuplicates: boolean;
}

export interface AppSnapshot {
  state: RecordingState;
  nextCaptureAt: string | null;
  todayCount: number;
  localStorageBytes: number;
  pendingUploads: number;
  cloudEnabled: boolean;
  permissionGranted: boolean;
  permissionState: PermissionState;
  lastError: string | null;
  settings: CaptureSettings;
}

export interface TimelineCapture {
  id: string;
  capturedAtUtc: string;
  cipherSize: number;
}

export interface TimelinePageResult {
  items: TimelineCapture[];
  nextOffset: number | null;
}

export type PageId = "today" | "timeline" | "privacy" | "storage";
