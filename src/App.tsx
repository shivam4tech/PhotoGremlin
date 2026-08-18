import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { TopBar } from "@/components/TopBar";
import { useAppStore } from "@/stores/appStore";
import { api, onProgress, toErrorMessage } from "@/lib/ipc";
import type {
  AnalysisCompletePayload,
  FaceCompletePayload,
  MetadataCompletePayload,
  OperationCompletePayload,
  ProgressPayload,
  ScanCompletePayload,
  SimilarityCompletePayload,
} from "@/types/api";
import { formatFaceSummaryLine } from "@/features/settings/ai";
import { isTypingTarget, shortcutFor } from "@/features/shortcuts";
import { LibraryView } from "@/views/LibraryView";
import { DashboardView } from "@/views/DashboardView";
import { SessionsView } from "@/views/SessionsView";
import { CollectionsView } from "@/views/CollectionsView";
import { SavedViewsView } from "@/views/SavedViewsView";
import { SettingsView } from "@/views/SettingsView";
import { VIEW_META } from "@/stores/appStore";

/**
 * Global keyboard shortcuts (Sprint 10): ⌘/Ctrl+O opens a photo folder,
 * bare 1–6 switch views. The viewer adds Esc/arrows with its own listener
 * (no key conflict). Never fires while the user is typing.
 */
function useAppShortcuts() {
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (isTypingTarget(e.target as HTMLElement | null)) return;
      const action = shortcutFor(e);
      if (!action) return;
      e.preventDefault();
      const s = useAppStore.getState();
      if (action.kind === "open-folder") {
        s.openFolder().catch((err) => s.setError(toErrorMessage(err)));
      } else {
        s.setView(action.view);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}

export default function App() {
  const view = useAppStore((s) => s.view);
  const error = useAppStore((s) => s.error);
  const notice = useAppStore((s) => s.notice);
  const setNotice = useAppStore((s) => s.setNotice);

  // Success notices are set by the pass-complete handlers and should be
  // transient: auto-dismiss after a few seconds, or immediately on ×.
  useEffect(() => {
    if (!notice) return;
    const t = window.setTimeout(() => setNotice(null), 8000);
    return () => window.clearTimeout(t);
  }, [notice, setNotice]);

  useAppShortcuts();

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
      const upm = await onProgress<ProgressPayload>("metadata-progress", (p) => {
        state().setProgress(p);
        state().setReadingMetadata(true);
      });
      const ucm = await onProgress<MetadataCompletePayload>("metadata-complete", (p) => {
        const s = state();
        s.setReadingMetadata(false);
        s.setProgress(null);
        if (p.summary) {
          s.setMetadataSummary(p.summary);
          if (p.summary.failed > 0 || (p.summary.processed > 0 && !p.summary.cancelled)) {
            s.setNotice(
              `Read metadata from ${p.summary.processed.toLocaleString()} photograph` +
                `${p.summary.processed === 1 ? "" : "s"}${p.summary.failed > 0 ? `, ${p.summary.failed.toLocaleString()} unreadable` : ""}.`,
            );
            s.setError(null);
          }
        } else {
          s.setMetadataSummary(null);
          s.setError(p.error ?? "Reading metadata failed.");
        }
        void s.refreshStatus();
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
        // Pipeline: as soon as a scan lands new photographs, read their
        // camera metadata in the background (a no-op when nothing is new).
        if (p.summary && p.summary.indexed > 0) {
          s.setReadingMetadata(true);
          s.setProgress({ total: 0, done: 0, stage: "reading metadata", current: null });
          api
            .startMetadata()
            .catch(() => {
              const st = useAppStore.getState();
              st.setReadingMetadata(false);
              st.setProgress(null);
            });
          // …and, when the user turned local intelligence on, detect faces
          // in the new photographs too (a no-op when nothing is queued).
          if (useAppStore.getState().aiEnabled) {
            api.startFaces().catch(() => {
              const st = useAppStore.getState();
              st.setDetectingFaces(false);
              st.setFacesProgress(null);
            });
          }
        }
      });
      const uop = await onProgress<ProgressPayload>("operation-progress", (p) => {
        const s = state();
        s.setOperating(true);
        s.setOpProgress(p);
      });
      const uoc = await onProgress<OperationCompletePayload>("operation-complete", (p) => {
        const s = state();
        s.setOperating(false);
        s.setOpProgress(null);
        if (p.summary) {
          const sum = p.summary;
          const verb =
            sum.op === "rename" ? "renamed" :
            sum.op === "move" ? "moved" :
            sum.op === "copy" ? "copied" : "trashed";
          const bits: string[] = [
            `${sum.succeeded.toLocaleString()} photograph${sum.succeeded === 1 ? "" : "s"} ${verb}`,
            sum.failed > 0 ? `${sum.failed.toLocaleString()} failed` : null,
            `${(sum.elapsed_ms / 1000).toFixed(1)}s`,
          ].filter(Boolean) as string[];
          s.setOpSummary(sum);
          s.setNotice(
            sum.cancelled
              ? `Operation stopped — ${bits.join(", ")}.`
              : `Operation complete — ${bits.join(", ")}.`,
          );
          s.setError(null);
        } else {
          s.setOpSummary(null);
          s.setError(p.error ?? "The file operation failed.");
        }
        // Files on disk changed: refresh counts, culling state, audit log,
        // and the grid (paths changed / rows removed).
        void s.refreshStatus();
        void s.loadSelections();
        void s.refreshRecentOps();
        s.bumpLibraryVersion();
      });
       const usim = await onProgress<ProgressPayload>("similarity-progress", (p) => {
         const s = state();
         s.setFindingSimilar(true);
         s.setSimilarityProgress(p);
       });
       const usimc = await onProgress<SimilarityCompletePayload>("similarity-complete", (p) => {
         const s = state();
         s.setFindingSimilar(false);
         s.setSimilarityProgress(null);
         if (p.summary) {
           const sum = p.summary;
           const bits: string[] = [
             `${sum.similar_groups.toLocaleString()} similar group${sum.similar_groups === 1 ? "" : "s"}`,
             `${sum.burst_groups.toLocaleString()} burst group${sum.burst_groups === 1 ? "" : "s"}`,
           ];
           if (sum.hashed > 0) bits.unshift(`${sum.hashed.toLocaleString()} hashed`);
           if (sum.failed > 0) bits.push(`${sum.failed.toLocaleString()} unreadable`);
           bits.push(`${(sum.elapsed_ms / 1000).toFixed(1)}s`);
           s.setSimilaritySummary(sum);
           s.setNotice(
             (sum.cancelled ? "Similarity pass stopped — " : "Similarity complete — ") +
               bits.join(", ") +
               ".",
           );
           s.setError(null);
         } else {
           s.setSimilaritySummary(null);
           s.setError(p.error ?? "Finding similar photos failed.");
         }
          // The group set changed: refresh it.
          void s.loadSimilarityGroups();
        });
       const uf = await onProgress<ProgressPayload>("faces-progress", (p) => {
         const s = state();
         s.setDetectingFaces(true);
         s.setFacesProgress(p);
       });
       const ufc = await onProgress<FaceCompletePayload>("faces-complete", (p) => {
         const s = state();
         s.setDetectingFaces(false);
         s.setFacesProgress(null);
         if (p.summary) {
           s.setFacesSummary(p.summary);
           if (p.summary.processed > 0 || p.summary.failed > 0 || !p.summary.cancelled) {
             s.setNotice(formatFaceSummaryLine(p.summary));
             s.setError(null);
           }
         } else {
           s.setFacesSummary(null);
           s.setError(p.error ?? "Face detection failed.");
         }
         void s.refreshStatus();
         void s.loadAiStatus();
       });
        return [up, upa, uca, upm, ucm, uc, uop, uoc, usim, usimc, uf, ufc];
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
        {notice && (
          <div
            role="status"
            style={{
              padding: "8px 34px 8px 20px",
              background: "var(--accent-soft)",
              color: "var(--text)",
              borderTop: "1px solid var(--accent-border)",
              fontSize: 12.5,
              position: "relative",
            }}
          >
            {notice}
            <button
              aria-label="Dismiss"
              onClick={() => setNotice(null)}
              style={{
                position: "absolute",
                right: 8,
                top: "50%",
                transform: "translateY(-50%)",
                background: "none",
                border: "none",
                color: "var(--text-dim)",
                cursor: "pointer",
                fontSize: 14,
                lineHeight: 1,
                padding: 4,
              }}
            >
              ×
            </button>
          </div>
        )}
        {error && (
          <div
            role="alert"
            style={{
            padding: "8px 20px",
            background: "var(--danger-soft)",
            color: "var(--danger)",
            borderTop: "1px solid var(--danger-border)",
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
