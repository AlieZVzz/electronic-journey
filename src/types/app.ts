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
  skipDuplicates: boolean;
}

export interface AppSnapshot {
  state: RecordingState;
  nextCaptureAt: string | null;
  todayCount: number;
  localStorageBytes: number;
  pendingAiJobs: number;
  permissionGranted: boolean;
  permissionState: PermissionState;
  lastError: string | null;
  settings: CaptureSettings;
}

export interface TimelineCapture {
  id: string;
  capturedAtUtc: string;
  fileSize: number;
}

export interface TimelinePageResult {
  items: TimelineCapture[];
  nextOffset: number | null;
}

export type PageId = "today" | "timeline" | "privacy" | "storage";
