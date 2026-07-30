export type RecordingState =
  | "stopped"
  | "running"
  | "paused"
  | "suspended"
  | "degraded";

export type PermissionState = "not_determined" | "granted" | "denied";

export type SuspensionReason =
  | "screen_locked"
  | "system_sleeping"
  | "user_idle";

export interface CaptureSettings {
  intervalMinutes: number;
  idlePauseMinutes: number;
}

export interface AppSnapshot {
  state: RecordingState;
  suspensionReason: SuspensionReason | null;
  nextCaptureAt: string | null;
  todayCount: number;
  localStorageBytes: number;
  pendingUploads: number;
  permissionGranted: boolean;
  permissionState: PermissionState;
  lastError: string | null;
  settings: CaptureSettings;
}

export interface TimelineCapture {
  id: string;
  capturedAtUtc: string;
  fileSize: number;
  uploadState:
    | "not_uploaded"
    | "pending"
    | "uploading"
    | "uploaded"
    | "failed"
    | "cancelled";
}

export interface RemoteProfile {
  name: string;
  host: string;
  port: number;
  username: string;
  privateKeyPath: string;
  hostKeyFingerprint: string;
  remoteRoot: string;
  hasPassphrase: boolean;
  autoSyncEnabled: boolean;
  syncIntervalMinutes: number;
  nextAutoSyncAtUtc: string | null;
  lastAutoSyncAttemptAtUtc: string | null;
  lastAutoSyncState:
    | "running"
    | "completed"
    | "partial_failed"
    | "empty"
    | "skipped_busy"
    | "suspended"
    | null;
  lastAutoSyncCompletedItems: number;
  lastAutoSyncFailedItems: number;
  autoSyncSuspendedReason: string | null;
}

export interface SaveRemoteProfileInput
  extends Omit<
    RemoteProfile,
    | "hasPassphrase"
    | "nextAutoSyncAtUtc"
    | "lastAutoSyncAttemptAtUtc"
    | "lastAutoSyncState"
    | "lastAutoSyncCompletedItems"
    | "lastAutoSyncFailedItems"
    | "autoSyncSuspendedReason"
  > {
  privateKeyPassphrase: string | null;
}

export interface RemoteConnectionTest {
  remoteRoot: string;
  writable: boolean;
}

export type UploadBatchState =
  | "pending"
  | "uploading"
  | "completed"
  | "partial_failed"
  | "cancelled";

export interface UploadItemProgress {
  captureId: string;
  state: TimelineCapture["uploadState"];
}

export interface UploadBatchProgress {
  batchId: string;
  state: UploadBatchState;
  totalItems: number;
  totalBytes: number;
  uploadedItems: number;
  failedItems: number;
  items: UploadItemProgress[];
  lastError: string | null;
}

export interface TimelinePageResult {
  items: TimelineCapture[];
  nextOffset: number | null;
}

export interface TimelineSelectionItem {
  id: string;
  fileSize: number;
}

export type PageId = "today" | "timeline" | "privacy" | "storage";
