import { useEffect } from "react";
import { useAppStore, VIEW_META } from "@/stores/appStore";
import { toErrorMessage } from "@/lib/ipc";
import { filterToDraft, toggleExactFieldCondition } from "@/features/library/filterFields";
import type { Filter, FilterCondition, SavedView, ViewId } from "@/types/api";
import {
  CollectionsIcon,
  DashboardIcon,
  FolderIcon,
  HomeIcon,
  LibraryIcon,
  LogoMark,
  SavedViewsIcon,
  SessionsIcon,
  SettingsIcon,
  LockIcon,
} from "./Icons";

const MANAGEMENT_NAV: { id: ViewId; icon: (p: { size?: number }) => JSX.Element }[] = [
  { id: "dashboard", icon: DashboardIcon },
  { id: "sessions", icon: SessionsIcon },
  { id: "collections", icon: CollectionsIcon },
  { id: "saved-views", icon: SavedViewsIcon },
];

const REVIEW_VIEWS: { label: string; condition: FilterCondition }[] = [
  { label: "Unreviewed", condition: { field: "review_state", operator: "is-null", value: null } },
  { label: "Kept", condition: { field: "review_state", operator: "=", value: "selected" } },
  { label: "Needs attention", condition: { field: "review_state", operator: "=", value: "needs_attention" } },
];

function sameCondition(left: FilterCondition | undefined, right: FilterCondition): boolean {
  return !!left && left.operator === right.operator && left.value === right.value;
}

export function Sidebar() {
  const view = useAppStore((s) => s.view);
  const activeFolder = useAppStore((s) => s.activeFolder);
  const filterConditions = useAppStore((s) => s.filterConditions);
  const recentProjects = useAppStore((s) => s.recentProjects);
  const savedViews = useAppStore((s) => s.savedViews);
  const setView = useAppStore((s) => s.setView);
  const setFilterConditions = useAppStore((s) => s.setFilterConditions);
  const loadSavedViews = useAppStore((s) => s.loadSavedViews);
  const openProject = useAppStore((s) => s.openProject);
  const openFolder = useAppStore((s) => s.openFolder);
  const setError = useAppStore((s) => s.setError);

  useEffect(() => {
    if (activeFolder) void loadSavedViews();
  }, [activeFolder, loadSavedViews]);

  const activeReview = filterConditions.find((condition) => condition.field === "review_state");
  const activeName = activeFolder?.split(/[\\/]/).filter(Boolean).pop() ?? "Current folder";
  const otherProjects = recentProjects.filter((project) => project.path !== activeFolder).slice(0, 4);

  function toggleReview(condition: FilterCondition) {
    setFilterConditions(toggleExactFieldCondition(filterConditions, condition));
    setView("library");
  }

  function openSavedView(savedView: SavedView) {
    try {
      setFilterConditions(filterToDraft(JSON.parse(savedView.filter_json) as Filter));
      setView("library");
    } catch {
      setError("This saved view's filter could not be read.");
    }
  }

  async function openRecent(path: string) {
    try {
      await openProject(path);
    } catch (error) {
      setError(toErrorMessage(error));
    }
  }

  return (
    <aside className="sidebar">
      <button className="brand" onClick={() => setView("home")} aria-label="PhotoGremlin home">
        <LogoMark size={28} />
        <div className="brand-name">
          Photo<span className="gremlin">Gremlin</span>
        </div>
      </button>

      <div className="sidebar-scroll">
        {activeFolder && (
          <section className="sidebar-section sidebar-review" aria-labelledby="sidebar-review-heading">
            <div className="sidebar-heading" id="sidebar-review-heading">Review views</div>
            <div className="sidebar-list">
              {REVIEW_VIEWS.map(({ label, condition }) => (
                <button
                  key={label}
                  className={`sidebar-row${sameCondition(activeReview, condition) ? " active" : ""}`}
                  onClick={() => toggleReview(condition)}
                  aria-pressed={sameCondition(activeReview, condition)}
                >
                  <span className="sidebar-review-dot" aria-hidden="true" />
                  <span className="sidebar-row-label">{label}</span>
                </button>
              ))}
            </div>
          </section>
        )}

        <section className="sidebar-section" aria-labelledby="sidebar-folders-heading">
          <div className="sidebar-heading-row">
            <div className="sidebar-heading" id="sidebar-folders-heading">Folders</div>
            <button
              className="sidebar-add"
              onClick={() => void openFolder().catch((error) => setError(toErrorMessage(error)))}
              aria-label="Open another photo folder"
              title="Open another photo folder"
            >
              +
            </button>
          </div>
          <div className="sidebar-list">
            {activeFolder && (
              <button
                className={`sidebar-row${view === "library" ? " active" : ""}`}
                onClick={() => setView("library")}
                title={activeFolder}
              >
                <FolderIcon size={15} />
                <span className="sidebar-row-label">{activeName}</span>
              </button>
            )}
            {otherProjects.map((project) => (
              <button
                className="sidebar-row"
                key={project.path}
                onClick={() => void openRecent(project.path)}
                title={project.path}
              >
                <FolderIcon size={15} />
                <span className="sidebar-row-label">{project.name}</span>
              </button>
            ))}
            {!activeFolder && otherProjects.length === 0 && (
              <span className="sidebar-empty">No recent folders</span>
            )}
          </div>
        </section>

        <section className="sidebar-section" aria-labelledby="sidebar-views-heading">
          <div className="sidebar-heading" id="sidebar-views-heading">Views</div>
          <div className="sidebar-list">
            {activeFolder && (
              <button
                className={`sidebar-row${view === "library" && filterConditions.length === 0 ? " active" : ""}`}
                onClick={() => { setFilterConditions([]); setView("library"); }}
              >
                <LibraryIcon size={15} />
                <span className="sidebar-row-label">All photographs</span>
              </button>
            )}
            {(savedViews ?? []).slice(0, 5).map((savedView) => (
              <button
                className="sidebar-row"
                key={savedView.id}
                onClick={() => openSavedView(savedView)}
                title={savedView.description ?? savedView.name}
              >
                <SavedViewsIcon size={15} />
                <span className="sidebar-row-label">{savedView.name}</span>
              </button>
            ))}
            {activeFolder && savedViews?.length === 0 && (
              <span className="sidebar-empty">No saved views</span>
            )}
          </div>
        </section>

        <nav className="sidebar-section" aria-labelledby="sidebar-manage-heading">
          <div className="sidebar-heading" id="sidebar-manage-heading">Manage</div>
          <div className="sidebar-list">
            {MANAGEMENT_NAV.map(({ id, icon: Icon }) => (
              <button
                key={id}
                className={`sidebar-row${view === id ? " active" : ""}`}
                onClick={() => setView(id)}
                title={VIEW_META[id].description}
              >
                <Icon size={15} />
                <span className="sidebar-row-label">{VIEW_META[id].label}</span>
              </button>
            ))}
          </div>
        </nav>
      </div>

      <nav className="sidebar-utility" aria-label="Application">
        {([
          { id: "home" as const, icon: HomeIcon },
          { id: "settings" as const, icon: SettingsIcon },
        ]).map(({ id, icon: Icon }) => (
          <button
            key={id}
            className={`sidebar-row${view === id ? " active" : ""}`}
            onClick={() => setView(id)}
            title={VIEW_META[id].description}
          >
            <Icon size={15} />
            <span className="sidebar-row-label">{VIEW_META[id].label}</span>
          </button>
        ))}
      </nav>

      <div className="sidebar-footer">
        <div className="privacy-badge">
          <span className="dot" />
          <LockIcon size={13} />
          <span>Local only · no cloud</span>
        </div>
      </div>
    </aside>
  );
}
