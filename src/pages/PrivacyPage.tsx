import { useState } from "react";

import { captureIntervals, validateCaptureSettings } from "../lib/settings";
import type { CaptureSettings } from "../types/app";

interface PrivacyPageProps {
  settings: CaptureSettings;
  loading: boolean;
  onSave: (settings: CaptureSettings) => Promise<void>;
}

export function PrivacyPage({
  settings,
  loading,
  onSave,
}: PrivacyPageProps) {
  const [draft, setDraft] = useState(settings);
  const errors = validateCaptureSettings(draft);

  return (
    <section className="placeholder-page">
      <h1>隐私中心</h1>
      <p>控制记录频率、空闲策略与图片处理默认值。</p>

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

        <label>
          <span>WebP 质量</span>
          <input
            max={100}
            min={1}
            onChange={(event) =>
              setDraft({
                ...draft,
                webpQuality: Number(event.target.value),
              })
            }
            type="number"
            value={draft.webpQuality}
          />
        </label>

        <label className="toggle-row">
          <span>
            <strong>跳过重复画面</strong>
            <small>重复检测仅在本机执行</small>
          </span>
          <input
            checked={draft.skipDuplicates}
            onChange={(event) =>
              setDraft({ ...draft, skipDuplicates: event.target.checked })
            }
            type="checkbox"
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
          disabled={loading || errors.length > 0}
          onClick={() => onSave(draft)}
          type="button"
        >
          保存设置
        </button>
      </div>
    </section>
  );
}
