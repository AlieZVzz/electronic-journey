import { describe, expect, it } from "vitest";

import {
  trayActionAvailability,
  trayStatusPresentation,
} from "./trayPanel";

describe("tray panel presentation", () => {
  it("keeps running and suspended controls aligned with the Rust state machine", () => {
    expect(trayActionAvailability("running")).toEqual({
      start: false,
      pause: true,
      stop: true,
    });
    expect(trayActionAvailability("suspended")).toEqual({
      start: false,
      pause: true,
      stop: true,
    });
  });

  it("allows paused recording to resume or stop", () => {
    expect(trayActionAvailability("paused")).toEqual({
      start: true,
      pause: false,
      stop: true,
    });
  });

  it("shows the specific system suspension reason", () => {
    expect(
      trayStatusPresentation({
        state: "suspended",
        suspensionReason: "screen_locked",
      }),
    ).toMatchObject({ label: "系统暂挂", detail: "屏幕已锁定" });
  });
});
