import { describe, expect, it } from "vitest";

import type { UploadBatchProgress } from "../types/app";
import {
  activeUploadProgressMessage,
  uploadDiagnosticsSummary,
} from "./uploadProgress";

function progress(): UploadBatchProgress {
  return {
    batchId: "batch-id",
    state: "uploading",
    totalItems: 4,
    totalBytes: 20 * 1024 * 1024,
    uploadedItems: 1,
    failedItems: 0,
    items: [],
    lastError: null,
    performance: {
      phase: "transferring",
      uploadedBytes: 8 * 1024 * 1024,
      bytesPerSecond: 2 * 1024 * 1024,
      estimatedRemainingSeconds: 6,
      connectionMs: 120,
      authenticationMs: 80,
      sftpInitializationMs: 40,
      localValidationMs: 300,
      transferBytes: 8 * 1024 * 1024,
      transferMs: 4_000,
      remoteMetadataOperations: 5,
      remoteMetadataMs: 250,
    },
  };
}

describe("upload progress formatting", () => {
  it("shows stage, bytes, speed, ETA, and item progress", () => {
    expect(activeUploadProgressMessage(progress())).toBe(
      "正在传输：8.0 MiB / 20.0 MiB · 2.0 MiB/s · 预计剩余 6 秒 · 1 / 4 张",
    );
  });

  it("formats aggregate diagnostics without paths or content identifiers", () => {
    const summary = uploadDiagnosticsSummary(progress());
    expect(summary).toContain("连接 120 ms");
    expect(summary).toContain("本地校验 300 ms");
    expect(summary).toContain("传输 4.0 s / 2.0 MiB/s");
    expect(summary).toContain("远端状态 5 次 / 250 ms");
    expect(summary).not.toContain("batch-id");
  });

  it("omits speed and ETA before bytes begin transferring", () => {
    const value = progress();
    value.performance.phase = "connecting";
    value.performance.uploadedBytes = 0;
    value.performance.bytesPerSecond = 0;
    value.performance.estimatedRemainingSeconds = null;
    expect(activeUploadProgressMessage(value)).toBe(
      "正在连接服务器：0.0 MiB / 20.0 MiB · 1 / 4 张",
    );
  });
});
