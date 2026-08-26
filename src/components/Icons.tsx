/** Minimal inline SVG icon set (no external icon dependency). */

interface IconProps {
  size?: number;
}

function svg(size: number, children: React.ReactNode, viewBox = "0 0 24 24") {
  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

export function LogoMark({ size = 26 }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 100 100" aria-hidden="true">
      <rect x="6" y="6" width="88" height="88" rx="20" fill="#1d2024" />
      <circle cx="34" cy="42" r="10.5" fill="#4ade80" />
      <circle cx="66" cy="42" r="10.5" fill="#4ade80" />
      <path
        d="M28 62 Q50 82 72 62"
        stroke="#4ade80"
        strokeWidth="8"
        fill="none"
        strokeLinecap="round"
      />
    </svg>
  );
}

export const LibraryIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <rect x="3" y="3" width="8" height="8" rx="1.5" />
      <rect x="13" y="3" width="8" height="8" rx="1.5" />
      <rect x="3" y="13" width="8" height="8" rx="1.5" />
      <rect x="13" y="13" width="8" height="8" rx="1.5" />
    </>,
  );

export const DashboardIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M4 20V10" />
      <path d="M10 20V4" />
      <path d="M16 20v-7" />
      <path d="M22 20H2" />
    </>,
  );

export const SessionsIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <rect x="3" y="5" width="18" height="16" rx="2" />
      <path d="M3 10h18" />
      <path d="M8 3v4M16 3v4" />
    </>,
  );

export const CollectionsIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    </>,
  );

export const SavedViewsIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M12 3l8 4-8 4-8-4z" />
      <path d="M4 12l8 4 8-4" />
    </>,
  );

export const SettingsIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h0a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h0a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v0a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1z" />
    </>,
  );

export const FolderIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  );

export const LockIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <>
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </>,
  );

export const ShieldIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <path d="M12 3l8 3v6c0 4.5-3.2 7.8-8 9-4.8-1.2-8-4.5-8-9V6z" />,
  );

export const SunIcon = ({ size = 15 }: IconProps) =>
  svg(
    size,
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
    </>,
  );

export const MoonIcon = ({ size = 15 }: IconProps) =>
  svg(
    size,
    <path d="M21 12.8A8.5 8.5 0 1 1 11.2 3 6.6 6.6 0 0 0 21 12.8z" />,
  );

export const HomeIcon = ({ size = 18 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M3 9.5L12 3l9 6.5V20a1 1 0 0 1-1 1h-4a1 1 0 0 1-1-1v-5H9v5a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1z" />
    </>,
  );

export const CloseIcon = ({ size = 14 }: IconProps) =>
  svg(size, <path d="M6 6l12 12M18 6L6 18" />);

export const MoreIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <>
      <circle cx="12" cy="12" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="6" cy="12" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="18" cy="12" r="1.2" fill="currentColor" stroke="none" />
    </>,
  );

export const GripIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <>
      <circle cx="8" cy="7" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="8" cy="12" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="8" cy="17" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="13" cy="7" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="13" cy="12" r="1.2" fill="currentColor" stroke="none" />
      <circle cx="13" cy="17" r="1.2" fill="currentColor" stroke="none" />
    </>,
  );

export const EyeIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M1 12s4-5.5 11-5.5S23 12 23 12s-4 5.5-11 5.5S1 12 1 12z" />
      <circle cx="12" cy="12" r="2.5" />
    </>,
  );

export const EyeOffIcon = ({ size = 14 }: IconProps) =>
  svg(
    size,
    <>
      <path d="M3 3l18 18" />
      <path d="M10.6 10.6A3 3 0 0 0 12 15a3 3 0 0 0 2.4-4.4" />
      <path d="M9.9 5.2A10.8 10.8 0 0 1 12 4.5C19 4.5 23 12 23 12a16.8 16.8 0 0 1-3.3 4" />
      <path d="M14.8 14.8A6.9 6.9 0 0 1 12 17.5C5 17.5 1 12 1 12a16.8 16.8 0 0 1 5.1-5.4" />
    </>,
  );
