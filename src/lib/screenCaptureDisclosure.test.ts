import { describe, expect, it } from "vitest";

import {
  canRequestScreenCapturePermission,
  screenCaptureDisclosure,
} from "./screenCaptureDisclosure";

describe("screen capture disclosure", () => {
  it("requires an explicit acknowledgement before requesting permission", () => {
    expect(canRequestScreenCapturePermission(false, false)).toBe(false);
    expect(canRequestScreenCapturePermission(true, true)).toBe(false);
    expect(canRequestScreenCapturePermission(true, false)).toBe(true);
  });

  it("discloses direct screen access and the absence of audio capture", () => {
    expect(screenCaptureDisclosure.introduction).toContain("程序化屏幕访问");
    expect(screenCaptureDisclosure.introduction).toContain("私密窗口选择器");
    expect(screenCaptureDisclosure.screenAccess).toContain("私人或敏感信息");
    expect(screenCaptureDisclosure.audio).toContain("不采集、保存或上传");
  });
});
