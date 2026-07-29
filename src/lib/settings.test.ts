import { describe, expect, it } from "vitest";

import {
  defaultCaptureSettings,
  validateCaptureSettings,
} from "./settings";

describe("capture settings", () => {
  it("accepts the product defaults", () => {
    expect(validateCaptureSettings(defaultCaptureSettings)).toEqual([]);
  });

  it("rejects unsupported and unsafe values", () => {
    expect(
      validateCaptureSettings({
        intervalMinutes: 3,
        idlePauseMinutes: -1,
        skipDuplicates: false,
      }),
    ).toHaveLength(2);
  });
});
