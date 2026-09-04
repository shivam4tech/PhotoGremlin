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
  const view = useAppStore((s) => s.view);
  const activeFolder = useAppStore((s) => s.activeFolder);
  const currentViewCount = useAppStore((s) => s.currentViewCount);
  const currentViewTotal = useAppStore((s) => s.currentViewTotal);
  const dbStatus = useAppStore((s) => s.dbStatus);

  // In the library with a project open, show live filtered count from the grid.
  // currentViewCount is set by LibraryView from useFilteredPhotos; it tracks
  // the session-scoped filtered total and updates on every filter/pagination change.
  const showLibraryLive = view === "library" && !!activeFolder && currentViewCount !== null;
  const count = showLibraryLive ? currentViewCount! : (dbStatus?.photo_count ?? 0);
  const countTitle = showLibraryLive
    ? (currentViewTotal !== null && currentViewTotal !== count
        ? `${count.toLocaleString()} of ${currentViewTotal!.toLocaleString()} in this project (filtered)`
        : `${count.toLocaleString()} in this project`)
    : "Indexed photographs (all projects)";

  return (
    <header className="topbar">
      <div>
        <h1>{title}</h1>
        {subtitle && <div className="faint" style={{ fontSize: 11.5 }}>{subtitle}</div>}
      </div>
      <div className="spacer" />
      {count > 0 ? (
        <span className="badge badge-accent mono" title={countTitle}>
          {count.toLocaleString()} photos
        </span>
      ) : null}
      {children}
    </header>
  );
}
