import { useEffect, useMemo, useRef, useState } from "react";
import { useAppStore } from "@/stores/appStore";
import { EmptyState } from "@/components/EmptyState";
import { FolderIcon } from "@/components/Icons";
import { api, toErrorMessage } from "@/lib/ipc";
import type { RecentProject } from "@/types/api";

function formatLastOpened(v: string | null): string {
  if (!v) return "—";
  const d = new Date(v);
  if (Number.isNaN(d.getTime())) return v;
  const diff = Date.now() - d.getTime();
  if (diff < 0) return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} hour${hrs === 1 ? "" : "s"} ago`;
  const days = Math.floor(hrs / 24);
  if (days < 7) return `${days} day${days === 1 ? "" : "s"} ago`;
  if (days < 30) return `${Math.floor(days / 7)} week${Math.floor(days / 7) === 1 ? "" : "s"} ago`;
  return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

export function HomeView() {
  const recentProjects = useAppStore((s) => s.recentProjects);
  const openProject = useAppStore((s) => s.openProject);
  const newProject = useAppStore((s) => s.newProject);
  const removeRecentProject = useAppStore((s) => s.removeRecentProject);
  const openInFileManager = useAppStore((s) => s.openInFileManager);

  const [menuOpen, setMenuOpen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [heroError, setHeroError] = useState<string | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);

  const projects = useMemo(() => (recentProjects ?? []).slice(0, 12), [recentProjects]);

  useEffect(() => {
    if (menuOpen === null) return;
    function onDown(e: MouseEvent) {
      const target = e.target as HTMLElement;
      if (menuRef.current?.contains(target)) return;
      const trigger = target.closest("[data-menu-trigger]");
      if (trigger?.getAttribute("data-menu-trigger") === menuOpen) return;
      setMenuOpen(null);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        const esc = typeof CSS !== "undefined" && typeof CSS.escape === "function" ? CSS.escape(menuOpen!) : menuOpen!.replace(/[^a-zA-Z0-9]/g, "\\$&");
        const trigger = document.querySelector<HTMLButtonElement>(`[data-menu-trigger="${esc}"]`);
        setMenuOpen(null);
        trigger?.focus();
      }
    }
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [menuOpen]);

  const isEmpty = projects.length === 0;

  async function handleOpenProject(path: string) {
    setHeroError(null);
    try {
      await openProject(path);
    } catch (e) {
      setHeroError(toErrorMessage(e));
    }
  }

  async function handleHeroOpen() {
    setBusy(true);
    setHeroError(null);
    try {
      const picked = await api.pickFolder();
      if (picked) await openProject(picked);
    } catch (e) {
      setHeroError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleHeroNew() {
    setBusy(true);
    setHeroError(null);
    try {
      await newProject();
    } catch (e) {
      setHeroError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="home">
      <section className="home-hero" aria-labelledby="home-title">
        <div className="home-hero-copy">
          <h1 id="home-title" className="home-hero-title">Your photos, on this machine</h1>
          <p className="home-hero-subtitle">
            PhotoGremlin keeps everything local — scanning, thumbnails, and analysis run here. Open a
            project to start, or create a new one.
          </p>
        </div>
        <div className="home-actions">
          <button className="btn btn-primary" onClick={() => void handleHeroOpen()} disabled={busy} aria-label="Open project">
            <FolderIcon size={16} />
            {busy ? "Opening…" : "Open project…"}
          </button>
          <button className="btn" onClick={() => void handleHeroNew()} disabled={busy} aria-label="New project">
            New project…
          </button>
        </div>
        {heroError ? <div role="alert" className="home-error">{heroError}</div> : null}
      </section>

      <section className="home-recents-section" aria-labelledby="home-recents-title">
        <div className="home-recents-head">
          <h2 id="home-recents-title" className="home-section-title">Recent projects</h2>
          {!isEmpty ? (
            <span className="home-recents-count mono" aria-live="polite">
              {projects.length} project{projects.length === 1 ? "" : "s"}
            </span>
          ) : null}
        </div>

        {isEmpty ? (
          <EmptyState
            glyph={<FolderIcon size={40} />}
            title="No recent projects yet — open a folder to get started."
            action={
              <button className="btn btn-primary" onClick={() => void handleHeroOpen()} disabled={busy}>
                Open project…
              </button>
            }
          >
            <p>Nothing is uploaded, ever.</p>
            <p className="faint" style={{ marginTop: 8, fontSize: 12.5 }}>Your recent projects will appear here — up to the last 12.</p>
          </EmptyState>
        ) : (
          <ul className="home-recents" role="list" aria-label="Recent projects">
            {projects.map((p: RecentProject) => {
              const isMenuOpen = menuOpen === p.path;
              return (
                <li key={p.path} className="home-row" role="listitem">
                  <button className="home-row-main" onClick={() => void handleOpenProject(p.path)} title={p.path} aria-label={`Open project ${p.name}`}>
                    <span className="home-row-text">
                      <span className="home-row-name" title={p.name}>{p.name}</span>
                      <span className="home-row-parent" title={p.parent}>{p.parent}</span>
                    </span>
                    <span className="home-row-meta">
                      <span className="home-row-time" title={p.lastOpenedAt}>{formatLastOpened(p.lastOpenedAt)}</span>
                      <span className="home-count mono" aria-label={`${p.photoCount} photographs`}>
                        {p.photoCount.toLocaleString()} photos
                      </span>
                    </span>
                  </button>
                  <div className="home-row-actions">
                    <button
                      className="home-row-menu"
                      aria-label={`Actions for ${p.name}`}
                      aria-haspopup="menu"
                      aria-expanded={isMenuOpen}
                      data-menu-trigger={p.path}
                      onClick={(e) => { e.stopPropagation(); setMenuOpen(isMenuOpen ? null : p.path); }}
                      onKeyDown={(e) => { if (e.key === "ArrowDown" && !isMenuOpen) { e.preventDefault(); setMenuOpen(p.path); } }}
                    >
                      <span aria-hidden="true">⋯</span>
                    </button>
                    {isMenuOpen ? (
                      <div className="home-menu" role="menu" aria-label={`Actions for ${p.name}`} ref={menuRef}>
                        <button role="menuitem" className="home-menu-item" onClick={() => { setMenuOpen(null); void openInFileManager(p.path); }}>
                          Open in file manager
                        </button>
                        <button role="menuitem" className="home-menu-item home-menu-item--danger" onClick={() => { setMenuOpen(null); void removeRecentProject(p.path); }}>
                          Remove from list
                        </button>
                      </div>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>
      <p className="home-privacy faint">Nothing is uploaded, ever. Everything stays on this machine.</p>
    </div>
  );
}

export default HomeView;
