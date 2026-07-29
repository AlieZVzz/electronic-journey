import { describe, expect, it } from "vitest";

import {
  captureSettingsEqual,
  recordingActionLabel,
  recordingSuccessMessage,
} from "./interactionFeedback";

describe("interaction feedback", () => {
  it("describes the recording action that is actually pending", () => {
    expect(recordingActionLabel("stopped", "running")).toBe("正在开始…");
    expect(recordingActionLabel("running", "paused")).toBe("正在暂停…");
    expect(recordingActionLabel("paused", "stopped")).toBe("正在停止…");
  });

  it("explains the verified recording result", () => {
    expect(recordingSuccessMessage("running")).toContain("10 秒");
    expect(recordingSuccessMessage("paused")).toContain("不会再安排");
    expect(recordingSuccessMessage("stopped")).toContain("不会被删除");
  });

  it("only treats identical capture settings as saved", () => {
    const saved = {
      intervalMinutes: 5,
      idlePauseMinutes: 10,
    };

    expect(captureSettingsEqual(saved, { ...saved })).toBe(true);
    expect(
      captureSettingsEqual(saved, { ...saved, intervalMinutes: 15 }),
    ).toBe(false);
  });
});
