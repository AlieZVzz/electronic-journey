import { describe, expect, it } from "vitest";

import { groupTimelineCaptures } from "./timeline";

describe("timeline grouping", () => {
  it("groups captures by the selected display timezone", () => {
    const captures = [
      {
        id: "one",
        capturedAtUtc: "2026-07-28T15:30:00Z",
        cipherSize: 10,
      },
      {
        id: "two",
        capturedAtUtc: "2026-07-28T16:30:00Z",
        cipherSize: 20,
      },
    ];

    expect(groupTimelineCaptures(captures, "Asia/Singapore")).toHaveLength(2);
    expect(groupTimelineCaptures(captures, "UTC")).toHaveLength(1);
  });

  it("preserves newest-first item ordering within each date", () => {
    const captures = [
      {
        id: "newer",
        capturedAtUtc: "2026-07-28T12:00:00Z",
        cipherSize: 10,
      },
      {
        id: "older",
        capturedAtUtc: "2026-07-28T10:00:00Z",
        cipherSize: 20,
      },
    ];

    expect(groupTimelineCaptures(captures, "UTC")[0].items.map(({ id }) => id)).toEqual([
      "newer",
      "older",
    ]);
  });
});
