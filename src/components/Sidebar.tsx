import type { ReactNode } from "react";

import type { PageId } from "../types/app";

const navigation: Array<{ id: PageId; label: string; icon: ReactNode }> = [
  {
    id: "today",
    label: "今日",
    icon: (
      <svg viewBox="0 0 20 20">
        <path d="M4 8.5A6 6 0 1 1 10 16H4V8.5Z" />
        <path d="M7 11h6M10 8v6" />
      </svg>
    ),
  },
  {
    id: "timeline",
    label: "时间线",
    icon: (
      <svg viewBox="0 0 20 20">
        <path d="M5 4.5h10M5 10h10M5 15.5h10" />
        <circle cx="7" cy="4.5" r="1.5" />
        <circle cx="12.5" cy="10" r="1.5" />
        <circle cx="9" cy="15.5" r="1.5" />
      </svg>
    ),
  },
  {
    id: "privacy",
    label: "隐私中心",
    icon: (
      <svg viewBox="0 0 20 20">
        <path d="M10 2.8 15.5 5v4.3c0 3.7-2.3 6.4-5.5 7.9-3.2-1.5-5.5-4.2-5.5-7.9V5L10 2.8Z" />
        <path d="m7.5 10 1.6 1.6 3.5-3.7" />
      </svg>
    ),
  },
  {
    id: "storage",
    label: "远程存储",
    icon: (
      <svg viewBox="0 0 20 20">
        <ellipse cx="10" cy="5" rx="6" ry="2.5" />
        <path d="M4 5v5c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5V5M4 10v5c0 1.4 2.7 2.5 6 2.5s6-1.1 6-2.5v-5" />
      </svg>
    ),
  },
];

interface SidebarProps {
  activePage: PageId;
  onNavigate: (page: PageId) => void;
}

export function Sidebar({ activePage, onNavigate }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar__drag-region" data-tauri-drag-region />
      <p className="sidebar__section-label">资料库</p>
      <nav aria-label="主导航">
        {navigation.map((item) => (
          <button
            className={activePage === item.id ? "nav-item is-active" : "nav-item"}
            key={item.id}
            onClick={() => onNavigate(item.id)}
            type="button"
          >
            <span className="nav-item__icon" aria-hidden="true">{item.icon}</span>
            <span>{item.label}</span>
          </button>
        ))}
      </nav>

      <div className="sidebar__privacy">
        <span className="sidebar__privacy-icon" aria-hidden="true">
          <svg viewBox="0 0 20 20">
            <rect x="4" y="8" width="12" height="9" rx="2" />
            <path d="M7 8V6a3 3 0 0 1 6 0v2" />
          </svg>
        </span>
        <div>
          <strong>本地资料库</strong>
          <p>仅本机 · 不自动上传</p>
        </div>
      </div>
    </aside>
  );
}
