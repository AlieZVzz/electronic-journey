interface IconProps {
  className?: string;
}

function iconClassName(className?: string) {
  return `app-icon${className ? ` ${className}` : ""}`;
}

export function CloseIcon({ className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={iconClassName(className)}
      focusable="false"
      viewBox="0 0 20 20"
    >
      <path d="m5.5 5.5 9 9m0-9-9 9" />
    </svg>
  );
}

export function PlusIcon({ className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={iconClassName(className)}
      focusable="false"
      viewBox="0 0 20 20"
    >
      <path d="M10 4.5v11M4.5 10h11" />
    </svg>
  );
}

export function StarIcon({ active = false }: IconProps & { active?: boolean }) {
  return (
    <svg
      aria-hidden="true"
      className="app-icon"
      focusable="false"
      viewBox="0 0 20 20"
    >
      <path
        d="m10 2.9 2.15 4.36 4.81.7-3.48 3.39.82 4.79L10 13.88l-4.3 2.26.82-4.79-3.48-3.39 4.81-.7L10 2.9Z"
        fill={active ? "currentColor" : "none"}
      />
    </svg>
  );
}

export function WarningIcon({ className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={iconClassName(className)}
      focusable="false"
      viewBox="0 0 20 20"
    >
      <path d="M10 3.2 17 16H3L10 3.2Z" />
      <path d="M10 7.3v4.4M10 14.25v.1" />
    </svg>
  );
}

export function ApplicationIcon({ className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={iconClassName(className)}
      focusable="false"
      viewBox="0 0 20 20"
    >
      <rect height="13" rx="2.25" width="15" x="2.5" y="3.5" />
      <path d="M3 7h14M6 5.25h.1M8 5.25h.1" />
    </svg>
  );
}

export function TrashIcon({ className }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className={iconClassName(className)}
      focusable="false"
      viewBox="0 0 20 20"
    >
      <path d="M4.5 6h11M8 3.75h4M6.25 6l.6 10h6.3l.6-10M8.25 8.5v5M11.75 8.5v5" />
    </svg>
  );
}
