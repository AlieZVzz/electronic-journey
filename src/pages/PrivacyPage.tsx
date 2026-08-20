import { useEffect, useState } from "react";

import { desktopApi } from "../api/desktop";
import { SelectControl } from "../components/SelectControl";
import {
  captureSettingsEqual,
  type RuntimeAction,
} from "../lib/interactionFeedback";
import { captureIntervals, validateCaptureSettings } from "../lib/settings";
import type { CaptureMode, CaptureSettings, PrivacyAppRule } from "../types/app";

const captureIntervalOptions = captureIntervals.map((interval) => ({
  label: `每 ${interval} 分钟`,
  value: interval,
}));

const captureModeOptions: ReadonlyArray<{
  label: string;
  value: CaptureMode;
}> = [
  { label: "所有显示器", value: "all" },
  { label: "当前使用的显示器", value: "active" },
];

interface PrivacyPageProps {
  settings: CaptureSettings;
  launchAtLogin: boolean;
  launchAtLoginSupported: boolean;
  loading: boolean;
  pendingAction: RuntimeAction | null;
  lastCaptureNotice: string | null;
  onSave: (settings: CaptureSettings) => Promise<void>;
  onLaunchAtLoginChange: (enabled: boolean) => Promise<void>;
}

export function PrivacyPage({
  settings,
  launchAtLogin,
  launchAtLoginSupported,
  loading,
  pendingAction,
  lastCaptureNotice,
  onSave,
  onLaunchAtLoginChange,
}: PrivacyPageProps) {
  const [draft, setDraft] = useState(settings);
  const errors = validateCaptureSettings(draft);
  const dirty = !captureSettingsEqual(draft, settings);
  const saving = pendingAction === "settings";
  const updatingLaunchAtLogin = pendingAction === "autostart";
  const [privacyRules, setPrivacyRules] = useState<PrivacyAppRule[]>([]);
  const [privacyRuleError, setPrivacyRuleError] = useState<string | null>(null);
  const [capturingApplicationIn, setCapturingApplicationIn] = useState<number | null>(null);

  useEffect(() => {
    void desktopApi
      .listPrivacyAppRules()
      .then(setPrivacyRules)
      .catch((reason) => setPrivacyRuleError(String(reason)));
  }, []);

  async function addFrontmostApplication() {
    setPrivacyRuleError(null);
    try {
      for (let remaining = 3; remaining > 0; remaining -= 1) {
        setCapturingApplicationIn(remaining);
        await new Promise((resolve) => window.setTimeout(resolve, 1000));
      }
      setCapturingApplicationIn(0);
      const rule = await desktopApi.addFrontmostPrivacyAppRule();
      setPrivacyRules((current) => [
        ...current.filter((item) => item.id !== rule.id),
        rule,
      ].sort((left, right) => left.displayName.localeCompare(right.displayName)));
    } catch (reason) {
      setPrivacyRuleError(String(reason));
    } finally {
      setCapturingApplicationIn(null);
    }
  }

  async function togglePrivacyRule(rule: PrivacyAppRule) {
    setPrivacyRuleError(null);
    try {
      await desktopApi.setPrivacyAppRuleEnabled(rule.id, !rule.enabled);
      setPrivacyRules((current) =>
        current.map((item) =>
          item.id === rule.id ? { ...item, enabled: !item.enabled } : item,
        ),
      );
    } catch (reason) {
      setPrivacyRuleError(String(reason));
    }
  }

  async function removePrivacyRule(rule: PrivacyAppRule) {
    setPrivacyRuleError(null);
    try {
      await desktopApi.deletePrivacyAppRule(rule.id);
      setPrivacyRules((current) => current.filter((item) => item.id !== rule.id));
    } catch (reason) {
      setPrivacyRuleError(String(reason));
    }
  }

  return (
    <section className="placeholder-page">
      <header className="page-header">
        <div>
          <p className="eyebrow">LOCAL · PRIVACY CONTROLS</p>
          <h1>隐私中心</h1>
          <p>控制记录频率与空闲策略；原图始终按捕获分辨率无损保存。</p>
        </div>
      </header>

      <div className="settings-panel">
        <section className="privacy-app-rules">
          <div>
            <strong>隐私应用排除</strong>
            <small>
              排除应用处于前台时，整次截图会跳过；不会保存或上传该周期的图片。
            </small>
          </div>
          {lastCaptureNotice && <p className="settings-panel__hint">{lastCaptureNotice}</p>}
          <button
            className="button button--ghost"
            disabled={!launchAtLoginSupported || capturingApplicationIn !== null}
            onClick={() => void addFrontmostApplication()}
            type="button"
          >
            {capturingApplicationIn === null
              ? "3 秒后读取前台应用"
              : capturingApplicationIn > 0
                ? `请切换到目标应用（${capturingApplicationIn}）`
                : "正在识别…"}
          </button>
          <small>点击后立即切换到要排除的应用；Electronic Journey 不保存窗口标题或进程路径。</small>
          {privacyRules.length === 0 ? (
            <p className="settings-panel__hint">尚未添加隐私应用。</p>
          ) : (
            <ul className="privacy-app-rules__list">
              {privacyRules.map((rule) => (
                <li key={rule.id}>
                  <span>
                    <strong>{rule.displayName}</strong>
                    <small>{rule.platform === "macos" ? "macOS" : "Windows"}</small>
                  </span>
                  <label>
                    <input
                      checked={rule.enabled}
                      onChange={() => void togglePrivacyRule(rule)}
                      type="checkbox"
                    />
                    启用
                  </label>
                  <button
                    className="button button--ghost"
                    onClick={() => void removePrivacyRule(rule)}
                    type="button"
                  >
                    移除
                  </button>
                </li>
              ))}
            </ul>
          )}
          {privacyRuleError && <p className="form-errors" role="alert">{privacyRuleError}</p>}
        </section>

        <label>
          <span>截图间隔</span>
          <SelectControl
            ariaLabel="截图间隔"
            onChange={(intervalMinutes) =>
              setDraft({
                ...draft,
                intervalMinutes,
              })
            }
            options={captureIntervalOptions}
            value={draft.intervalMinutes}
          />
        </label>

        <label>
          <span>
            捕获范围
            <small>
              当前显示器按当前前台窗口所在的显示器判断；无法判断时使用主显示器。
            </small>
          </span>
          <SelectControl<CaptureMode>
            ariaLabel="捕获范围"
            onChange={(captureMode) =>
              setDraft({
                ...draft,
                captureMode,
              })
            }
            options={captureModeOptions}
            value={draft.captureMode}
          />
        </label>

        <label>
          <span>
            空闲暂停
            <small>连续空闲达到该分钟数后暂停；设为 0 可关闭。</small>
          </span>
          <input
            max={240}
            min={0}
            onChange={(event) =>
              setDraft({
                ...draft,
                idlePauseMinutes: Number(event.target.value),
              })
            }
            type="number"
            value={draft.idlePauseMinutes}
          />
        </label>

        <label className="toggle-row">
          <span>
            <strong>开机自动启动</strong>
            <small>
              登录 macOS 或 Windows 后在后台启动；不会自动开始截图。
            </small>
          </span>
          <input
            checked={launchAtLogin}
            disabled={
              loading || updatingLaunchAtLogin || !launchAtLoginSupported
            }
            onChange={(event) =>
              void onLaunchAtLoginChange(event.currentTarget.checked)
            }
            type="checkbox"
          />
        </label>

        {!launchAtLoginSupported && (
          <p className="settings-panel__hint">
            开机自启动仅支持 macOS 和 Windows 桌面应用。
          </p>
        )}

        {errors.length > 0 && (
          <ul className="form-errors">
            {errors.map((error) => (
              <li key={error}>{error}</li>
            ))}
          </ul>
        )}

        <button
          className="button button--primary"
          aria-busy={saving}
          disabled={loading || errors.length > 0 || !dirty}
          onClick={() => void onSave(draft)}
          type="button"
        >
          {saving ? "正在保存…" : dirty ? "保存设置" : "设置已保存"}
        </button>
      </div>
    </section>
  );
}
