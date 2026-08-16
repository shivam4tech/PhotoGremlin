import type { ReactNode } from "react";
import { useAppStore } from "@/stores/appStore";

export function TopBar({
  title,
  subtitle,
  children,
}: {
  title: string;
  subtitle?: string;
  children?: ReactNode;
}) {
  const dbStatus = useAppStore((s) => s.dbStatus);
  return (
    <header className="topbar">
      <div>
        <h1>{title}</h1>
        {subtitle && <div className="faint" style={{ fontSize: 11.5 }}>{subtitle}</div>}
      </div>
      <div className="spacer" />
      {dbStatus && dbStatus.photo_count > 0 ? (
        <span className="badge badge-accent mono" title="Indexed photographs">
          {dbStatus.photo_count.toLocaleString()} photos
        </span>
      ) : null}
      {children}
    </header>
  );
}
