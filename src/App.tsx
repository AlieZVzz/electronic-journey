import { useState } from "react";

import { desktopApi } from "./api/desktop";
import { FirstRunOnboarding } from "./components/FirstRunOnboarding";
import { Sidebar } from "./components/Sidebar";
import {
  hasCompletedOnboarding,
  markOnboardingComplete,
} from "./lib/onboarding";
import { PrivacyPage } from "./pages/PrivacyPage";
import { StoragePage } from "./pages/StoragePage";
import { TimelinePage } from "./pages/TimelinePage";
import { TodayPage } from "./pages/TodayPage";
import { useAppRuntime } from "./stores/useAppRuntime";
import type { PageId } from "./types/app";

const pageTitles: Record<PageId, string> = {
  today: "今日",
  timeline: "时间线",
  privacy: "隐私中心",
  storage: "存储与安全",
};

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("today");
  const [onboardingComplete, setOnboardingComplete] = useState(() =>
    hasCompletedOnboarding(window.localStorage),
  );
  const {
    snapshot,
    loading,
    error,
    requestScreenCapturePermission,
    setRecordingState,
    updateSettings,
  } = useAppRuntime();
  const visibleError = error ?? snapshot?.lastError;
  const showOnboarding =
    desktopApi.isDesktopRuntime() && !onboardingComplete && snapshot !== null;

  function completeOnboarding() {
    if (!snapshot?.permissionGranted) {
      return;
    }

    markOnboardingComplete(window.localStorage);
    setOnboardingComplete(true);
  }

  return (
    <div className="app-shell">
      <Sidebar activePage={activePage} onNavigate={setActivePage} />
      <section className="workspace">
        <header className="titlebar" data-tauri-drag-region>
          <strong data-tauri-drag-region>{pageTitles[activePage]}</strong>
        </header>
        <main className="main-content">
          <div className="content-frame">
            {visibleError && <div className="error-banner">{visibleError}</div>}
            {!snapshot ? (
              <div className="loading-state">正在读取本机状态…</div>
            ) : (
              <>
                {activePage === "today" && (
                  <TodayPage
                    loading={loading}
                    onPermissionRequest={requestScreenCapturePermission}
                    onStateChange={setRecordingState}
                    snapshot={snapshot}
                  />
                )}
                {activePage === "timeline" && <TimelinePage />}
                {activePage === "privacy" && (
                  <PrivacyPage
                    loading={loading}
                    onSave={updateSettings}
                    settings={snapshot.settings}
                  />
                )}
                {activePage === "storage" && <StoragePage />}
              </>
            )}
          </div>
        </main>
      </section>
      {showOnboarding && snapshot && (
        <FirstRunOnboarding
          loading={loading}
          onComplete={completeOnboarding}
          onPermissionRequest={requestScreenCapturePermission}
          permissionState={snapshot.permissionState}
        />
      )}
    </div>
  );
}
