import { describe, expect, it } from "vitest";

import { appUpdateProgressPercent, appUpdateProgressText } from "./appUpdate";

describe("app update progress", () => {
  it("formats bounded determinate download progress", () => {
    const progress = {
      phase: "downloading" as const,
      downloadedBytes: 2 * 1024 * 1024,
      totalBytes: 4 * 1024 * 1024,
    };
    expect(appUpdateProgressText(progress)).toBe("已下载 2.0 MB / 4.0 MB");
    expect(appUpdateProgressPercent(progress)).toBe(50);
  });

  it("caps inconsistent server progress and handles installation", () => {
    expect(
      appUpdateProgressPercent({
        phase: "downloading",
        downloadedBytes: 3,
        totalBytes: 2,
      }),
    ).toBe(100);
    expect(
      appUpdateProgressText({
        phase: "installing",
        downloadedBytes: 10,
        totalBytes: 10,
      }),
    ).toContain("验证更新签名");
    expect(appUpdateProgressPercent(null)).toBeNull();
  });
});
