import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { TopBar } from "@/components/TopBar";
import { useAppStore } from "@/stores/appStore";
import { api, onProgress, toErrorMessage } from "@/lib/ipc";
import type {
  AnalysisCompletePayload,
  ProgressPayload,
  ScanCompletePayload,
} from "@/types/api";
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

  // Scan + analysis progress/completion stream in from the backend at all
  // times. The UI keeps the two exclusive (buttons disable each other), so
  // one shared progress field is honest.
  useEffect(() => {
    const unlisteners = (async () => {
      const state = () => useAppStore.getState();
      const up = await onProgress<ProgressPayload>("scan-progress", (p) => {
        state().setProgress(p);
        state().setScanning(true);
      });
      const upa = await onProgress<ProgressPayload>("analysis-progress", (p) => {
        state().setProgress(p);
        state().setAnalyzing(true);
      });
      const uca = await onProgress<AnalysisCompletePayload>("analysis-complete", (p) => {
        const s = state();
        s.setAnalyzing(false);
        s.setProgress(null);
        if (p.summary) {
          const sum = p.summary;
          const bits: string[] = [
            `${sum.analyzed.toLocaleString()} photograph${sum.analyzed === 1 ? "" : "s"} measured`,
            sum.failed > 0 ? `${sum.failed.toLocaleString()} failed` : null,
            `${(sum.elapsed_ms / 1000).toFixed(1)}s`,
          ].filter(Boolean) as string[];
          s.setAnalysisSummary(sum);
          s.setNotice(
            sum.cancelled
              ? `Analysis stopped — ${bits.join(", ")}.`
              : `Analysis complete — ${bits.join(", ")}.`,
          );
          s.setError(null);
        } else {
          s.setAnalysisSummary(null);
          s.setError(p.error ?? "Analysis failed.");
        }
        void s.refreshStatus();
      });
      const uc = await onProgress<ScanCompletePayload>("scan-complete", (p) => {
        const s = state();
        s.setScanning(false);
        s.setProgress(null);
        if (p.summary) {
          const msg = p.summary.cancelled
            ? `Scan stopped — indexed ${p.summary.indexed.toLocaleString()} of ${p.summary.total_files.toLocaleString()} files.`
            : `Scan complete — ${p.summary.indexed.toLocaleString()} photographs indexed into session “${p.summary.session_name}” in ${(p.summary.elapsed_ms / 1000).toFixed(1)}s${p.summary.ignored ? `, ${p.summary.ignored.toLocaleString()} non-photo files ignored` : ""}.`;
          s.setScanSummary(p.summary);
          s.setNotice(msg);
          s.setError(null);
        } else {
          s.setScanSummary(null);
          s.setError(p.error ?? "Scan failed.");
        }
        void s.refreshStatus();
      });
      return [up, upa, uca, uc];
    })();

    let alive = true;
    unlisteners.then((list) => {
      if (!alive) list.forEach((u) => u());
    });
    return () => {
      alive = false;
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
        <div className={view === "library" ? "view-scroll library-scroll" : "view-scroll"}>{body}</div>
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
