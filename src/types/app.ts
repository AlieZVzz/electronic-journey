export type RecordingState =
  | "stopped"
  | "running"
  | "paused"
  | "suspended"
  | "degraded";

export type PermissionState = "not_determined" | "granted" | "denied";

export type CaptureMode = "all" | "active";

export type SuspensionReason =
  | "screen_locked"
  | "system_sleeping"
  | "user_idle";

export interface CaptureSettings {
  intervalMinutes: number;
  idlePauseMinutes: number;
  captureMode: CaptureMode;
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
  lastCaptureNotice: string | null;
  settings: CaptureSettings;
  launchAtLogin: boolean;
}

export interface TrayMenuSnapshot {
  status: string;
  permission: string;
  todayCaptured: string;
  todayUploaded: string;
  permissionActionEnabled: boolean;
  startEnabled: boolean;
  pauseEnabled: boolean;
  stopEnabled: boolean;
}

export type TrayMenuAction =
  | "permission"
  | "start"
  | "pause"
  | "stop"
  | "open"
  | "quit"
  | "dismiss";

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
  favorite: boolean;
  tags: TimelineTag[];
}

export interface TimelineTag {
  id: string;
  name: string;
}

export interface PrivacyAppRule {
  id: string;
  platform: "macos" | "windows";
  displayName: string;
  enabled: boolean;
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

export type UploadPhase =
  | "pending"
  | "connecting"
  | "authenticating"
  | "initializing_sftp"
  | "validating_local"
  | "preparing_remote"
  | "transferring"
  | "verifying_remote"
  | "completed"
  | "failed";

export interface UploadPerformance {
  phase: UploadPhase;
  uploadedBytes: number;
  bytesPerSecond: number;
  estimatedRemainingSeconds: number | null;
  connectionMs: number;
  authenticationMs: number;
  sftpInitializationMs: number;
  localValidationMs: number;
  transferBytes: number;
  transferMs: number;
  remoteMetadataOperations: number;
  remoteMetadataMs: number;
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
  performance: UploadPerformance;
}

export interface TimelinePageResult {
  items: TimelineCapture[];
  nextOffset: number | null;
}

export interface TimelineSelectionItem {
  id: string;
  fileSize: number;
}

export interface AppUpdateInfo {
  currentVersion: string;
  version: string;
  notes: string | null;
  publishedAt: string | null;
}

export interface AppUpdateProgress {
  phase: "downloading" | "installing";
  downloadedBytes: number;
  totalBytes: number | null;
}

export type PageId = "today" | "timeline" | "privacy" | "storage" | "about";
