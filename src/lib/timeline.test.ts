import { describe, expect, it } from "vitest";

import {
  addTimelineSelection,
  groupTimelineCaptures,
} from "./timeline";

describe("timeline grouping", () => {
  it("groups captures by the selected display timezone", () => {
    const captures = [
      {
        id: "one",
        capturedAtUtc: "2026-07-28T15:30:00Z",
        fileSize: 10,
        uploadState: "not_uploaded" as const,
        favorite: false,
        tags: [],
      },
      {
        id: "two",
        capturedAtUtc: "2026-07-28T16:30:00Z",
        fileSize: 20,
        uploadState: "not_uploaded" as const,
        favorite: false,
        tags: [],
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
        fileSize: 10,
        uploadState: "uploaded" as const,
        favorite: true,
        tags: [],
      },
      {
        id: "older",
        capturedAtUtc: "2026-07-28T10:00:00Z",
        fileSize: 20,
        uploadState: "not_uploaded" as const,
        favorite: false,
        tags: [],
      },
    ];

    expect(groupTimelineCaptures(captures, "UTC")[0].items.map(({ id }) => id)).toEqual([
      "newer",
      "older",
    ]);
  });
});

describe("timeline day selection", () => {
  it("adds unloaded day items without discarding existing selections", () => {
    const current = new Map([["already-selected", 10]]);
    const next = addTimelineSelection(current, [
      { id: "loaded", fileSize: 20 },
      { id: "not-loaded", fileSize: 30 },
    ]);

    expect(Array.from(next.entries())).toEqual([
      ["already-selected", 10],
      ["loaded", 20],
      ["not-loaded", 30],
    ]);
    expect(Array.from(current.entries())).toEqual([
      ["already-selected", 10],
    ]);
  });
});
