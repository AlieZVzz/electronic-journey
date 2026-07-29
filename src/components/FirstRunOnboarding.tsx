import { useState } from "react";

import { canRequestScreenCapturePermission } from "../lib/screenCaptureDisclosure";
import type { AppSnapshot, PermissionState } from "../types/app";
import { ScreenCaptureDisclosure } from "./ScreenCaptureDisclosure";

interface FirstRunOnboardingProps {
  loading: boolean;
  permissionState: PermissionState;
  onComplete: () => void;
  onPermissionRequest: () => Promise<AppSnapshot>;
}

export function FirstRunOnboarding({
  loading,
  permissionState,
  onComplete,
  onPermissionRequest,
}: FirstRunOnboardingProps) {
  const [step, setStep] = useState<"privacy" | "permission">("privacy");
  const [permissionAcknowledged, setPermissionAcknowledged] = useState(false);
  const [directAccessVerified, setDirectAccessVerified] = useState(false);
  const permissionGranted = permissionState === "granted";
  const permissionReady = permissionGranted && directAccessVerified;

  async function requestAndVerifyPermission() {
    try {
      const nextSnapshot = await onPermissionRequest();
      setDirectAccessVerified(nextSnapshot.permissionGranted);
    } catch {
      setDirectAccessVerified(false);
    }
  }

  return (
    <div className="onboarding-backdrop">
      <section
        aria-labelledby="onboarding-title"
        aria-modal="true"
        className="onboarding-dialog"
        role="dialog"
      >
        <div className="onboarding-progress" aria-label="首次启动进度">
          <span className="is-current">1</span>
          <i />
          <span className={step === "permission" ? "is-current" : ""}>2</span>
        </div>

        {step === "privacy" ? (
          <>
            <p className="eyebrow">WELCOME · PRIVACY FIRST</p>
            <h1 id="onboarding-title">开始你的私人数字旅程</h1>
            <p className="onboarding-lead">
              Electronic Journey 只在你主动开启时记录屏幕。截图默认只保存在本机，
              默认不会上传；只有你之后配置个人服务器并明确开启自动同步，图片才会按计划离开本机。
            </p>

            <ul className="onboarding-promises">
              <li>
                <span>01</span>
                <div>
                  <strong>始终可见、随时可停</strong>
                  <p>不会静默录制；暂停或停止后不再安排新的截图。</p>
                </div>
              </li>
              <li>
                <span>02</span>
                <div>
                  <strong>系统权限由你决定</strong>
                  <p>下一步才会显示 macOS 或 Windows 的系统授权界面。</p>
                </div>
              </li>
              <li>
                <span>03</span>
                <div>
                  <strong>当前采用安全默认值</strong>
                  <p>首次记录会在开启 10 秒后执行；远程上传必须由你选择图片并确认。</p>
                </div>
              </li>
            </ul>

            <div className="onboarding-actions">
              <button
                className="button button--onboarding-primary"
                onClick={() => setStep("permission")}
                type="button"
              >
                我已了解，继续设置
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="eyebrow">SYSTEM PERMISSION</p>
            <h1 id="onboarding-title">授权前，请先了解访问方式</h1>
            <p className="onboarding-lead">
              下面的说明会在 macOS 系统警告之前显示。确认后才会发起屏幕录制权限请求。
            </p>

            {!permissionReady && (
              <ScreenCaptureDisclosure
                acknowledged={permissionAcknowledged}
                onAcknowledgementChange={setPermissionAcknowledged}
              />
            )}

            <div
              className={`permission-status permission-status--${permissionState}`}
            >
              <span aria-hidden="true" />
              <div>
                <strong>
                  {permissionGranted
                    ? directAccessVerified
                      ? "屏幕录制权限和直接访问均已就绪"
                      : "系统权限已开启，等待直接访问验证"
                    : permissionState === "denied"
                      ? "系统尚未授予权限"
                      : "等待你的授权"}
                </strong>
                <p>
                  {permissionReady
                    ? "已完成一次不落盘的访问验证，可以完成首次设置。"
                    : permissionGranted
                      ? "仍需确认上方说明并执行一次不保存、不上传的屏幕访问验证。"
                    : permissionState === "denied"
                      ? "请在系统设置中允许 Electronic Journey；系统可能要求重新启动应用。"
                      : "阅读并确认上方说明后，才能打开系统授权。"}
                </p>
              </div>
            </div>

            <div className="onboarding-actions">
              <button
                className="button button--onboarding-secondary"
                disabled={loading}
                onClick={() => setStep("privacy")}
                type="button"
              >
                返回
              </button>
              {!permissionReady && (
                <button
                  className="button button--onboarding-primary"
                  aria-busy={loading}
                  disabled={
                    !canRequestScreenCapturePermission(
                      permissionAcknowledged,
                      loading,
                    )
                  }
                  onClick={() => void requestAndVerifyPermission()}
                  type="button"
                >
                  {loading
                    ? "正在检查…"
                    : permissionGranted
                      ? "我理解，验证系统访问"
                      : "我理解，打开系统授权"}
                </button>
              )}
              {permissionReady && (
                <button
                  className="button button--onboarding-primary"
                  onClick={onComplete}
                  type="button"
                >
                  完成首次设置
                </button>
              )}
            </div>
          </>
        )}
      </section>
    </div>
  );
}
