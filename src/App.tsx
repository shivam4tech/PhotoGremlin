import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { TopBar } from "@/components/TopBar";
import { useAppStore } from "@/stores/appStore";
import { api, toErrorMessage } from "@/lib/ipc";
import { LibraryView } from "@/views/LibraryView";
import { DashboardView } from "@/views/DashboardView";
import { SessionsView } from "@/views/SessionsView";
import { CollectionsView } from "@/views/CollectionsView";
import { SavedViewsView } from "@/views/SavedViewsView";
import { SettingsView } from "@/views/SettingsView";
import { VIEW_META } from "@/stores/appStore";

export default function App() {
  const view = useAppStore((s) => s.view);
  const error = useAppStore((s) => s.error);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const { setAppInfo, setPaths, setError, refreshStatus } = useAppStore.getState();
      try {
        const [info, paths] = await Promise.all([api.appInfo(), api.appPaths()]);
        if (cancelled) return;
        setAppInfo(info);
        setPaths(paths);
      } catch (e) {
        if (!cancelled) setError(toErrorMessage(e));
        return;
      }
      await refreshStatus();
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const body = (() => {
    switch (view) {
      case "library":
        return <LibraryView />;
      case "dashboard":
        return <DashboardView />;
      case "sessions":
        return <SessionsView />;
      case "collections":
        return <CollectionsView />;
      case "saved-views":
        return <SavedViewsView />;
      case "settings":
        return <SettingsView />;
    }
  })();

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        <TopBar title={VIEW_META[view].label} subtitle={VIEW_META[view].description} />
        <div className="view-scroll">{body}</div>
        {error && (
          <div
            role="alert"
            style={{
              padding: "8px 20px",
              background: "var(--danger-soft)",
              color: "var(--danger)",
              borderTop: "1px solid rgba(248,113,113,0.3)",
              fontSize: 12.5,
            }}
          >
            {error}
          </div>
        )}
      </main>
    </div>
  );
}
