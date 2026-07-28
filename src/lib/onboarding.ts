export const onboardingStorageKey =
  "electronic-journey.onboarding.v1.complete";

interface OnboardingStorage {
  getItem: (key: string) => string | null;
  setItem: (key: string, value: string) => void;
}

export function hasCompletedOnboarding(storage: OnboardingStorage): boolean {
  try {
    return storage.getItem(onboardingStorageKey) === "true";
  } catch {
    return false;
  }
}

export function markOnboardingComplete(storage: OnboardingStorage): void {
  try {
    storage.setItem(onboardingStorageKey, "true");
  } catch {
    // The current session may continue safely. If local storage is unavailable,
    // the consent flow is intentionally shown again on the next launch.
  }
}
