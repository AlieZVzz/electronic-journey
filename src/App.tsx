import { useState } from "react";

import { desktopApi } from "./api/desktop";
import { CloseIcon } from "./components/AppIcons";
import { FirstRunOnboarding } from "./components/FirstRunOnboarding";
import { Sidebar } from "./components/Sidebar";
import { WindowControls } from "./components/WindowControls";
import {
  hasCompletedOnboarding,
  markOnboardingComplete,
} from "./lib/onboarding";
import { PrivacyPage } from "./pages/PrivacyPage";
import { AboutPage } from "./pages/AboutPage";
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
  about: "关于与更新",
};

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("today");
  const isWindowsDesktop = desktopApi.isWindowsDesktopRuntime();
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
    setLaunchAtLogin,
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
    <div
      className={`app-shell${isWindowsDesktop ? " app-shell--windows" : ""}`}
    >
      <Sidebar activePage={activePage} onNavigate={navigate} />
      <section className="workspace">
        <header
          className={`titlebar${isWindowsDesktop ? " titlebar--windows" : ""}`}
          data-tauri-drag-region
        >
          <strong data-tauri-drag-region>{pageTitles[activePage]}</strong>
          {isWindowsDesktop && <WindowControls />}
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
                  <CloseIcon />
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
                    launchAtLogin={snapshot.launchAtLogin}
                    launchAtLoginSupported={desktopApi.isDesktopRuntime()}
                    pendingAction={pendingAction}
                    lastCaptureNotice={snapshot.lastCaptureNotice}
                    onLaunchAtLoginChange={setLaunchAtLogin}
                    onSave={updateSettings}
                    settings={snapshot.settings}
                  />
                )}
                {activePage === "storage" && <StoragePage />}
                {activePage === "about" && <AboutPage />}
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
