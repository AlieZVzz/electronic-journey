import { useState } from "react";

import {
  captureSettingsEqual,
  type RuntimeAction,
} from "../lib/interactionFeedback";
import { captureIntervals, validateCaptureSettings } from "../lib/settings";
import type { CaptureSettings } from "../types/app";

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
          <select
            onChange={(event) =>
              setDraft({
                ...draft,
                intervalMinutes: Number(event.target.value),
              })
            }
            value={draft.intervalMinutes}
          >
            {captureIntervals.map((interval) => (
              <option key={interval} value={interval}>
                每 {interval} 分钟
              </option>
            ))}
          </select>
        </label>

        <label>
          <span>空闲暂停</span>
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
