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
  storage: "远程存储",
};

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("today");
  const [onboardingComplete, setOnboardingComplete] = useState(() =>
    hasCompletedOnboarding(window.localStorage),
  );
  const {
    snapshot,
    loading,
    pendingAction,
    recordingTarget,
    error,
    notice,
    dismissNotice,
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

  function navigate(page: PageId) {
    dismissNotice();
    setActivePage(page);
  }

  return (
    <div className="app-shell">
      <Sidebar activePage={activePage} onNavigate={navigate} />
      <section className="workspace">
        <header className="titlebar" data-tauri-drag-region>
          <strong data-tauri-drag-region>{pageTitles[activePage]}</strong>
        </header>
        <main className="main-content">
          <div className="content-frame">
            {visibleError && <div className="error-banner">{visibleError}</div>}
            {notice && (
              <div className="success-banner app-feedback" role="status">
                <span>{notice}</span>
                <button
                  aria-label="关闭操作提示"
                  onClick={dismissNotice}
                  type="button"
                >
                  ×
                </button>
              </div>
            )}
            {!snapshot ? (
              <div className="loading-state">正在读取本机状态…</div>
            ) : (
              <>
                {activePage === "today" && (
                  <TodayPage
                    loading={loading}
                    pendingAction={pendingAction}
                    recordingTarget={recordingTarget}
                    onPermissionRequest={requestScreenCapturePermission}
                    onStateChange={setRecordingState}
                    snapshot={snapshot}
                  />
                )}
                {activePage === "timeline" && <TimelinePage />}
                {activePage === "privacy" && (
                  <PrivacyPage
                    loading={loading}
                    pendingAction={pendingAction}
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
