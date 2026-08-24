import { screenCaptureDisclosure } from "../lib/screenCaptureDisclosure";
import { WarningIcon } from "./AppIcons";

interface ScreenCaptureDisclosureProps {
  acknowledged: boolean;
  onAcknowledgementChange: (acknowledged: boolean) => void;
}

export function ScreenCaptureDisclosure({
  acknowledged,
  onAcknowledgementChange,
}: ScreenCaptureDisclosureProps) {
  return (
    <>
      <div className="capture-disclosure">
        <div className="capture-disclosure__warning" aria-hidden="true">
          <WarningIcon />
        </div>
        <div>
          <strong>这不是每次都出现的窗口选择授权</strong>
          <p>{screenCaptureDisclosure.introduction}</p>
        </div>
      </div>

      <ul className="capture-disclosure__facts">
        <li>
          <strong>屏幕内容可能包含敏感信息</strong>
          <p>{screenCaptureDisclosure.screenAccess}</p>
        </li>
        <li>
          <strong>当前版本不录制音频</strong>
          <p>{screenCaptureDisclosure.audio}</p>
        </li>
      </ul>

      <label className="capture-disclosure__consent">
        <input
          checked={acknowledged}
          onChange={(event) =>
            onAcknowledgementChange(event.currentTarget.checked)
          }
          type="checkbox"
        />
        <span>
          我理解应用将直接访问屏幕内容，并确认当前版本不会采集音频。
        </span>
      </label>
    </>
  );
}
