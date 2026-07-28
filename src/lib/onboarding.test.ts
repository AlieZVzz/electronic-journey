import { describe, expect, it } from "vitest";

import {
  hasCompletedOnboarding,
  markOnboardingComplete,
  onboardingStorageKey,
} from "./onboarding";

function createStorage(initialValue: string | null = null) {
  let value = initialValue;

  return {
    getItem: (key: string) =>
      key === onboardingStorageKey ? value : null,
    setItem: (key: string, nextValue: string) => {
      if (key === onboardingStorageKey) {
        value = nextValue;
      }
    },
  };
}

describe("first-run onboarding state", () => {
  it("shows onboarding until it has been completed", () => {
    const storage = createStorage();

    expect(hasCompletedOnboarding(storage)).toBe(false);
    markOnboardingComplete(storage);
    expect(hasCompletedOnboarding(storage)).toBe(true);
  });

  it("does not treat unrelated or malformed values as completion", () => {
    expect(hasCompletedOnboarding(createStorage("false"))).toBe(false);
    expect(hasCompletedOnboarding(createStorage("1"))).toBe(false);
  });

  it("fails closed when storage is unavailable", () => {
    const unavailableStorage = {
      getItem: () => {
        throw new Error("storage unavailable");
      },
      setItem: () => {
        throw new Error("storage unavailable");
      },
    };

    expect(hasCompletedOnboarding(unavailableStorage)).toBe(false);
    expect(() => markOnboardingComplete(unavailableStorage)).not.toThrow();
  });
});
