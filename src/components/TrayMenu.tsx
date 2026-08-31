import { useCallback, useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { desktopApi } from "../api/desktop";
import type { TrayMenuAction, TrayMenuSnapshot } from "../types/app";

type IconName =
  | "status"
  | "permission"
  | "captured"
  | "uploaded"
  | "start"
  | "pause"
  | "stop"
  | "open"
  | "quit";

const iconPaths: Record<IconName, React.ReactNode> = {
  status: <><circle cx="10" cy="10" r="7" /><path d="M10 9.5v4M10 6.5v.1" /></>,
  permission: <><path d="M5.5 9V7a4.5 4.5 0 0 1 9 0v2" /><rect x="3.5" y="9" width="13" height="8" rx="2" /><path d="M10 12v2" /></>,
  captured: <><rect x="3" y="4" width="14" height="12" rx="2" /><path d="m5.5 13 3-3 2.25 2 1.75-1.5 2 2M13.5 7.5h.1" /></>,
  uploaded: <><path d="M10 14V5m0 0L6.5 8.5M10 5l3.5 3.5" /><path d="M4 13.5V16h12v-2.5" /></>,
  start: <path d="m7 5 8 5-8 5V5Z" />,
  pause: <><path d="M7 5v10M13 5v10" /></>,
  stop: <rect x="5" y="5" width="10" height="10" rx="1.5" />,
  open: <><rect x="3.5" y="4" width="13" height="12" rx="2" /><path d="M7 10h6M10 7l3 3-3 3" /></>,
  quit: <><path d="M10 3v7" /><path d="M5.4 5.5a7 7 0 1 0 9.2 0" /></>,
};

function TrayIcon({ name }: { name: IconName }) {
  return (
    <svg aria-hidden="true" className="tray-menu__icon" focusable="false" viewBox="0 0 20 20">
      {iconPaths[name]}
    </svg>
  );
}

interface MenuItemProps {
  action?: TrayMenuAction;
  disabled?: boolean;
  icon: IconName;
  label: string;
  muted?: boolean;
  onAction: (action: TrayMenuAction) => void;
}

function MenuItem({ action, disabled, icon, label, muted, onAction }: MenuItemProps) {
  return (
    <button
      className={`tray-menu__item${muted ? " tray-menu__item--muted" : ""}`}
      disabled={disabled || !action}
      onClick={() => action && onAction(action)}
      role="menuitem"
      title={label}
      type="button"
    >
      <TrayIcon name={icon} />
      <span>{label}</span>
    </button>
  );
}

export function TrayMenu() {
  const menuRef = useRef<HTMLDivElement>(null);
  const [snapshot, setSnapshot] = useState<TrayMenuSnapshot | null>(null);
  const [pending, setPending] = useState<TrayMenuAction | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await desktopApi.getTrayMenuSnapshot());
      setError(null);
      window.requestAnimationFrame(() => {
        menuRef.current
          ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
          ?.focus();
      });
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    document.documentElement.classList.add("tray-menu-page");
    void refresh();

    let active = true;
    let unlisten: UnlistenFn | undefined;
    void listen("tray-menu-opened", () => void refresh()).then((stopListening) => {
      if (active) unlisten = stopListening;
      else stopListening();
    });

    return () => {
      active = false;
      unlisten?.();
      document.documentElement.classList.remove("tray-menu-page");
    };
  }, [refresh]);

  async function runAction(action: TrayMenuAction) {
    if (pending) return;
    setPending(action);
    try {
      await desktopApi.runTrayMenuAction(action);
    } catch (reason) {
      setError(String(reason));
      setPending(null);
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      void runAction("dismiss");
      return;
    }
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;

    const items = Array.from(
      menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
    );
    if (!items.length) return;
    event.preventDefault();
    const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : event.key === "ArrowDown"
          ? (currentIndex + 1 + items.length) % items.length
          : (currentIndex - 1 + items.length) % items.length;
    items[nextIndex]?.focus();
  }

  const loadingLabel = "正在读取…";

  return (
    <div className="tray-menu__window">
      <div
        aria-label="Electronic Journey 托盘菜单"
        className="tray-menu"
        onKeyDown={handleKeyDown}
        ref={menuRef}
        role="menu"
      >
        {error && <div className="tray-menu__error" role="alert">暂时无法读取应用状态</div>}
        <MenuItem icon="status" label={snapshot?.status ?? loadingLabel} muted onAction={runAction} />
        <MenuItem
          action={snapshot?.permissionActionEnabled ? "permission" : undefined}
          icon="permission"
          label={snapshot?.permission ?? loadingLabel}
          muted={!snapshot?.permissionActionEnabled}
          onAction={runAction}
        />
        <MenuItem icon="captured" label={snapshot?.todayCaptured ?? loadingLabel} muted onAction={runAction} />
        <MenuItem icon="uploaded" label={snapshot?.todayUploaded ?? loadingLabel} muted onAction={runAction} />
        <div className="tray-menu__separator" role="separator" />
        <MenuItem action="start" disabled={!snapshot?.startEnabled || pending !== null} icon="start" label="开始记录" onAction={runAction} />
        <MenuItem action="pause" disabled={!snapshot?.pauseEnabled || pending !== null} icon="pause" label="暂停记录" onAction={runAction} />
        <MenuItem action="stop" disabled={!snapshot?.stopEnabled || pending !== null} icon="stop" label="停止记录" onAction={runAction} />
        <div className="tray-menu__separator" role="separator" />
        <MenuItem action="open" disabled={pending !== null} icon="open" label="打开主窗口" onAction={runAction} />
        <div className="tray-menu__separator" role="separator" />
        <MenuItem action="quit" disabled={pending !== null} icon="quit" label="退出 Electronic Journey" onAction={runAction} />
      </div>
    </div>
  );
}
