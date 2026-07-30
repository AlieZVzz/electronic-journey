import type {
  TimelineCapture,
  TimelineSelectionItem,
} from "../types/app";

export interface TimelineGroup {
  dateKey: string;
  label: string;
  items: TimelineCapture[];
}

export function addTimelineSelection(
  current: Map<string, number>,
  items: TimelineSelectionItem[],
): Map<string, number> {
  const next = new Map(current);
  for (const item of items) {
    next.set(item.id, item.fileSize);
  }
  return next;
}

function dateKey(value: string, timeZone?: string): string {
  const parts = new Intl.DateTimeFormat("en-CA", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    timeZone,
  }).formatToParts(new Date(value));
  const values = Object.fromEntries(parts.map((part) => [part.type, part.value]));
  return `${values.year}-${values.month}-${values.day}`;
}

export function groupTimelineCaptures(
  captures: TimelineCapture[],
  timeZone?: string,
): TimelineGroup[] {
  const groups = new Map<string, TimelineCapture[]>();
  for (const capture of captures) {
    const key = dateKey(capture.capturedAtUtc, timeZone);
    const group = groups.get(key);
    if (group) {
      group.push(capture);
    } else {
      groups.set(key, [capture]);
    }
  }

  return Array.from(groups, ([key, items]) => ({
    dateKey: key,
    label: new Intl.DateTimeFormat("zh-CN", {
      year: "numeric",
      month: "long",
      day: "numeric",
      weekday: "short",
      timeZone,
    }).format(new Date(items[0].capturedAtUtc)),
    items,
  }));
}
