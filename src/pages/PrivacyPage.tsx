import { useState } from "react";

import { SelectControl } from "../components/SelectControl";
import {
  captureSettingsEqual,
  type RuntimeAction,
} from "../lib/interactionFeedback";
import { captureIntervals, validateCaptureSettings } from "../lib/settings";
import type { CaptureMode, CaptureSettings } from "../types/app";

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
  loading: boolean;
  pendingAction: RuntimeAction | null;
  onSave: (settings: CaptureSettings) => Promise<void>;
}

export function PrivacyPage({
  settings,
  loading,
  pendingAction,
  onSave,
}: PrivacyPageProps) {
  const [draft, setDraft] = useState(settings);
  const errors = validateCaptureSettings(draft);
  const dirty = !captureSettingsEqual(draft, settings);
  const saving = pendingAction === "settings";

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
