import { useAppStore, VIEW_META } from "@/stores/appStore";
import type { ViewId } from "@/types/api";
import {
  CollectionsIcon,
  DashboardIcon,
  HomeIcon,
  LibraryIcon,
  LogoMark,
  SavedViewsIcon,
  SessionsIcon,
  SettingsIcon,
  LockIcon,
} from "./Icons";

const NAV: { id: ViewId; icon: (p: { size?: number }) => JSX.Element }[] = [
  { id: "home", icon: HomeIcon },
  { id: "library", icon: LibraryIcon },
  { id: "dashboard", icon: DashboardIcon },
  { id: "sessions", icon: SessionsIcon },
  { id: "collections", icon: CollectionsIcon },
  { id: "saved-views", icon: SavedViewsIcon },
  { id: "settings", icon: SettingsIcon },
];

export function Sidebar() {
  const view = useAppStore((s) => s.view);
  const setView = useAppStore((s) => s.setView);

  return (
    <aside className="sidebar">
      <div className="brand">
        <LogoMark size={28} />
        <div className="brand-name">
          Photo<span className="gremlin">Gremlin</span>
        </div>
      </div>

      <nav className="nav">
        {NAV.map(({ id, icon: Icon }) => (
          <button
            key={id}
            className={`nav-item${view === id ? " active" : ""}`}
            onClick={() => setView(id)}
            title={VIEW_META[id].description}
          >
            <Icon size={18} />
            {VIEW_META[id].label}
          </button>
        ))}
      </nav>

      <div className="sidebar-footer">
        <div className="privacy-badge">
          <span className="dot" />
          <span>
            Local only. Your photos never leave
            <br />
            this computer.
          </span>
        </div>
        <div className="privacy-badge" style={{ marginTop: 8 }}>
          <LockIcon size={13} />
          <span>No account. No cloud. No telemetry.</span>
        </div>
      </div>
    </aside>
  );
}
