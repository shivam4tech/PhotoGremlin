import { useEffect, useMemo, useRef, useState } from "react";
import { api, toErrorMessage } from "@/lib/ipc";
import { useAppStore } from "@/stores/appStore";
import { useFilteredPhotos } from "@/hooks/useFilteredPhotos";
import { EmptyState } from "@/components/EmptyState";
import { ProgressBar } from "@/components/ProgressBar";
import { VirtualGrid } from "@/components/VirtualGrid";
import { PhotoTile } from "@/components/PhotoTile";
import { Viewer } from "@/features/viewer/Viewer";
import { FilterBar } from "@/features/library/FilterBar";
import { FileOpsPanel } from "@/features/fileops/FileOpsPanel";
import { ExportSheetButton } from "@/features/library/ExportSheetButton";
import { MarksPanel } from "@/features/library/MarksPanel";
import { ReviewMode } from "@/features/review/ReviewMode";
import { CoverThumb } from "@/features/similarity/CoverThumb";
import { cleanName, groupLabel } from "@/features/organize/labels";
import { draftToFilter } from "@/features/library/filterFields";
import { FolderIcon } from "@/components/Icons";
import type { PhotoSummary, SimilarityGroup } from "@/types/api";

const GROUPS_PAGE = 12;
/** A group grid loads up to this many photos at once (groups are small). */
const GROUP_PAGE = 500;

export function LibraryView() {
  const activeFolder = useAppStore((s) => s.activeFolder);
  const dbStatus = useAppStore((s) => s.dbStatus);
  const scanning = useAppStore((s) => s.scanning);
  const progress = useAppStore((s) => s.progress);
  const scanSummary = useAppStore((s) => s.scanSummary);
  const analyzing = useAppStore((s) => s.analyzing);
  const analysisSummary = useAppStore((s) => s.analysisSummary);
  const readingMetadata = useAppStore((s) => s.readingMetadata);
  const metadataPaused = useAppStore((s) => s.metadataPaused);
  const metadataSummary = useAppStore((s) => s.metadataSummary);
  const operating = useAppStore((s) => s.operating);
  const selections = useAppStore((s) => s.selections);
  const selectionMode = useAppStore((s) => s.selectionMode);
  const libraryVersion = useAppStore((s) => s.libraryVersion);
  const filterConditions = useAppStore((s) => s.filterConditions);
  const collections = useAppStore((s) => s.collections);
  const findingSimilar = useAppStore((s) => s.findingSimilar);
  const similarityProgress = useAppStore((s) => s.similarityProgress);
  const similaritySummary = useAppStore((s) => s.similaritySummary);
  const similarityGroups = useAppStore((s) => s.similarityGroups);
  const store = useAppStore.getState;

  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [viewerId, setViewerId] = useState<number | null>(null);
  const [reviewMode, setReviewMode] = useState(false);

  // Saving the current filter as a named view.
  const [saveViewOpen, setSaveViewOpen] = useState(false);
  const [viewName, setViewName] = useState("");

  // Similar groups: the currently opened group (null = normal library grid).
  const [group, setGroup] = useState<SimilarityGroup | null>(null);
  const [groupPhotos, setGroupPhotos] = useState<PhotoSummary[]>([]);
  const [groupLoading, setGroupLoading] = useState(false);
  const [groupTab, setGroupTab] = useState<"all" | "similar" | "face" | "burst">("all");
  const [groupsVisible, setGroupsVisible] = useState(GROUPS_PAGE);
  useEffect(() => { setGroupsVisible(GROUPS_PAGE); }, [similarityGroups, groupTab]);

  // Re-fetch the index when a scan completes, a file operation changed the
  // files on disk, or the folder changed. Include activeFolder so switching
  // projects immediately shows the new project's photos.
  const refreshKey = useMemo(
    () => `${activeFolder ?? "none"}:${scanSummary && scanSummary.session_name ? scanSummary.session_name : "none"}:${libraryVersion}`,
    [activeFolder, scanSummary, libraryVersion],
  );

  // Load persisted culling state once per library so tiles render their marks.
  useEffect(() => {
    if (selectionMode) void store().loadSelections();
  }, [selectionMode, refreshKey]);
  const [sessionId, setSessionId] = useState<number | null>(null);
  const [sessionPhotoCount, setSessionPhotoCount] = useState<number | null>(null);
  const [sessionName, setSessionName] = useState("This shoot");

  // Resolve the current project's session so the grid shows only its photos.
  useEffect(() => {
    if (!activeFolder) { setSessionId(null); setSessionPhotoCount(null); setSessionName("This shoot"); return; }
    let cancelled = false;
    api.listSessions().then((rows) => {
      if (cancelled) return;
      const hit = rows.find((r) => r.root_path === activeFolder);
      setSessionId(hit ? hit.id : null);
      setSessionPhotoCount(hit ? hit.photo_count : null);
      setSessionName(hit?.name ?? "This shoot");
    }).catch(() => { if (!cancelled) { setSessionId(null); setSessionPhotoCount(null); setSessionName("This shoot"); } });
    return () => { cancelled = true; };
  }, [activeFolder, scanSummary, dbStatus?.photo_count]);

  const libraryHasPhotos = !!activeFolder && (dbStatus?.photo_count ?? 0) > 0;
  const filterJson = useMemo(() => {
    const base = draftToFilter(filterConditions);
    // When the folder has no session yet (before first scan), show nothing
    // rather than leaking in photos from other projects.
    if (sessionId === null && activeFolder) {
      return JSON.stringify({ operator: "AND", conditions: [{ field: "session_id", operator: "=", value: -1 }] });
    }
    if (sessionId === null) return JSON.stringify(base);
    const sessionCond = { field: "session_id", operator: "=", value: sessionId };
    if (typeof base === "string" && base === "") {
      return JSON.stringify({ operator: "AND", conditions: [sessionCond] });
    }
    const obj = base as { operator: string; conditions: unknown[] };
    return JSON.stringify({ ...obj, conditions: [...obj.conditions, sessionCond] });
  }, [filterConditions, sessionId, activeFolder]);
  const photos = useFilteredPhotos(
    libraryHasPhotos && group === null,
    filterJson,
    refreshKey,
  );

  // Publish live counts for the TopBar badge: filtered vs session total.
  // Group view is handled separately (its own count in the statusbar).
  const setCurrentViewCount = useAppStore((s) => s.setCurrentViewCount);
  useEffect(() => {
    if (!activeFolder || group !== null) {
      // Group view owns the statusbar; TopBar falls back to global.
      setCurrentViewCount(null, null);
      return;
    }
    // photos.total is session-scoped filtered count; sessionPhotoCount is the
    // unfiltered session total (null before the session row loads — fall back
    // to filtered total so the badge never shows 0 while loading).
    setCurrentViewCount(photos.total, sessionPhotoCount ?? photos.total);
    return () => { setCurrentViewCount(null, null); };
  }, [activeFolder, group, photos.total, sessionPhotoCount, setCurrentViewCount]);

  // Load the similarity group set once when it isn't known yet (so returning
  // to the Library shows the cards without re-running the pass).
  useEffect(() => {
    if (libraryHasPhotos && similarityGroups === null) {
      void store().loadSimilarityGroups();
    }
  }, [libraryHasPhotos, similarityGroups]);

  const anyPassRunning = scanning || analyzing || readingMetadata;
  const metadataPending = dbStatus?.metadata_pending ?? 0;

  async function openFolder() {
    setBusy(true);
    setError(null);
    try {
      // The store action is shared with the ⌘/Ctrl+O shortcut (Sprint 10).
      await store().openFolder();
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function startScan() {
    if (!activeFolder) return;
    setError(null);
    try {
      await api.startScan(activeFolder);
      store().setScanning(true);
      store().setProgress({ total: 0, done: 0, stage: "discovering", current: null });
      store().setScanSummary(null);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setScanning(false);
    }
  }

  async function stopScan() {
    try {
      await api.stopScan();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function startAnalysis() {
    setError(null);
    try {
      await api.startAnalysis();
      store().setAnalyzing(true);
      store().setProgress({ total: 0, done: 0, stage: "analyzing", current: null });
      store().setAnalysisSummary(null);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setAnalyzing(false);
    }
  }

  async function stopAnalysis() {
    try {
      await api.stopAnalysis();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function startMetadata() {
    setError(null);
    try {
      store().setMetadataSummary(null);
      store().setMetadataPaused(false);
      await api.startMetadata();
      store().setReadingMetadata(true);
      store().setProgress({ total: 0, done: 0, stage: "reading metadata", current: null });
    } catch (e) {
      setError(toErrorMessage(e));
      store().setReadingMetadata(false);
      store().setMetadataPaused(false);
    }
  }

  async function stopMetadata() {
    try {
      const stopping = await api.stopMetadata();
      if (stopping) {
        store().setMetadataPaused(false);
        const progress = useAppStore.getState().progress;
        store().setProgress({
          total: progress?.total ?? 0,
          done: progress?.done ?? 0,
          stage: "stopping metadata",
          current: progress?.current ?? null,
        });
      }
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function toggleMetadataPause() {
    try {
      const changed = metadataPaused ? await api.resumeMetadata() : await api.pauseMetadata();
      if (!changed) return;
      const nextPaused = !metadataPaused;
      store().setMetadataPaused(nextPaused);
      const progress = useAppStore.getState().progress;
      store().setProgress({
        total: progress?.total ?? 0,
        done: progress?.done ?? 0,
        stage: nextPaused ? "metadata paused" : "reading metadata",
        current: progress?.current ?? null,
      });
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function saveCurrentView() {
    const name = cleanName(viewName);
    if (name === null) {
      setError("View name must be 1–60 characters.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.saveView(name, filterJson, null);
      setViewName("");
      setSaveViewOpen(false);
      await store().loadSavedViews();
      store().setNotice(`Saved view “${name}” — reopen it from Saved Views.`);
    } catch (e) {
      setError(toErrorMessage(e));
    } finally {
      setBusy(false);
    }
  }

  async function startSimilarity() {
    setError(null);
    try {
      store().setSimilaritySummary(null);
      store().setSimilarityProgress({ total: 0, done: 0, stage: "hashing", current: null });
      await api.startSimilarity();
      store().setFindingSimilar(true);
    } catch (e) {
      setError(toErrorMessage(e));
      store().setFindingSimilar(false);
      store().setSimilarityProgress(null);
    }
  }

  async function stopSimilarity() {
    try {
      await api.stopSimilarity();
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  async function openGroup(g: SimilarityGroup) {
    setGroup(g);
    setViewerId(null);
    setGroupLoading(true);
    setError(null);
    try {
      const res = await api.groupPhotos(g.id, 0, GROUP_PAGE);
      setGroupPhotos(res.photos);
    } catch (e) {
      setError(toErrorMessage(e));
      setGroupPhotos([]);
    } finally {
      setGroupLoading(false);
    }
  }

  const hasPhotos = group !== null ? groupPhotos.length > 0 : photos.total > 0;

  // Culling: ids marked "selected" drive the file-operations panel.
  const selectedIds = useMemo(
    () => Object.keys(selections).filter((k) => selections[Number(k)] === "selected").map(Number),
    [selections],
  );
  const rejectedCount = useMemo(
    () => Object.values(selections).filter((s) => s === "rejected").length,
    [selections],
  );
  const pagePhotos = group !== null ? groupPhotos : photos.photos;
  const pageIds = useMemo(() => pagePhotos.map((p) => p.id), [pagePhotos]);

  async function addToCollection(collectionId: number) {
    if (selectedIds.length === 0) return;
    setError(null);
    try {
      const added = await api.addToCollection(collectionId, selectedIds);
      await store().loadCollections();
      store().setNotice(
        added === 0
          ? "Those photographs are already in that collection."
          : `Added ${added.toLocaleString()} photograph${added === 1 ? "" : "s"} to the collection.`,
      );
    } catch (e) {
      setError(toErrorMessage(e));
    }
  }

  function keep(id: number) {
    store().setSelection(id, "selected");
  }
  function reject(id: number) {
    store().setSelection(id, "rejected");
  }
  function clearSel(id: number) {
    store().setSelection(id, null);
  }

  // Forward wheel events from toolbar/filter areas to the photo grid
  function forwardWheel(e: React.WheelEvent) {
    const vg = document.querySelector(".vg") as HTMLDivElement | null;
    if (vg && e.deltaY !== 0) {
      const atTop = vg.scrollTop <= 0;
      const atBottom = vg.scrollTop + vg.clientHeight >= vg.scrollHeight - 1;
      if ((e.deltaY < 0 && !atTop) || (e.deltaY > 0 && !atBottom)) {
        vg.scrollTop += e.deltaY;
        e.preventDefault();
      }
    }
  }

  // Reset scroll when switching projects
  const prevFolderRef = useRef(activeFolder);
  useEffect(() => {
    if (prevFolderRef.current !== activeFolder) {
      prevFolderRef.current = activeFolder;
      setReviewMode(false);
      const vg = document.querySelector(".vg") as HTMLDivElement | null;
      if (vg) vg.scrollTop = 0;
    }
  }, [activeFolder]);

  if (!activeFolder) {
    return (
      <>
        <ErrorBanner message={error} />
        <EmptyState
          glyph={<FolderIcon size={40} />}
          title="Open a photo folder"
          action={
            <button className="btn btn-primary" onClick={openFolder} disabled={busy}>
              <FolderIcon size={16} />
              {busy ? "Opening…" : "Open Folder"}
            </button>
          }
        >
          Point PhotoGremlin at a folder of photographs. Everything —
          scanning, thumbnails, analysis, statistics — runs on this machine.
          Nothing is uploaded, ever.
        </EmptyState>
      </>
    );
  }

  if (reviewMode && sessionId !== null) {
    return <ReviewMode sessionId={sessionId} sessionName={sessionName} onClose={() => setReviewMode(false)} />;
  }

  return (
    <div className="library" onWheel={forwardWheel}>
      <div className="library-toolbar">
        <FolderIcon size={16} />
        <span className="mono library-toolbar-path" title={activeFolder}>{activeFolder}</span>
        <span className="spacer" />
        {!scanning ? (
          <button
            className="btn btn-sm btn-primary"
            onClick={startScan}
            disabled={busy || photos.loading || anyPassRunning || findingSimilar}
          >
            {dbStatus && dbStatus.photo_count > 0 ? "Re-scan" : "Scan folder"}
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopScan}>Stop scan</button>
        )}
        {!readingMetadata && metadataPending > 0 ? (
          <button
            className="btn btn-sm"
            onClick={startMetadata}
            disabled={scanning || analyzing || findingSimilar || busy || !libraryHasPhotos}
            title={`Read camera metadata (EXIF) for ${metadataPending.toLocaleString()} photograph${metadataPending === 1 ? "" : "s"} — never-read files plus any changed on disk since their last read.`}
          >
            Read metadata ({metadataPending.toLocaleString()})
          </button>
        ) : null}
        {readingMetadata ? (
          <>
            <button className="btn btn-sm" onClick={() => void toggleMetadataPause()}>
              {metadataPaused ? "Resume metadata" : "Pause metadata"}
            </button>
            <button className="btn btn-sm btn-danger" onClick={() => void stopMetadata()}>
              Stop reading
            </button>
          </>
        ) : null}
        {!analyzing ? (
          <button
            className="btn btn-sm"
            onClick={startAnalysis}
            disabled={anyPassRunning || findingSimilar || photos.total === 0 || busy}
            title="Measure every photo that still needs it: sharpness, brightness, contrast, saturation, clipping, monochrome. Re-runs are incremental."
          >
            Analyze photos
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={stopAnalysis}>
            Stop analysis
          </button>
        )}
        {!findingSimilar ? (
          <button
            className="btn btn-sm"
            onClick={() => void startSimilarity()}
            disabled={anyPassRunning || operating || photos.total === 0 || busy}
            title="Photograph every image with a perceptual hash, then group near-duplicates (similar) and same-moment runs (bursts). Re-runs only process changed or new files."
          >
            Find similar photos
          </button>
        ) : (
          <button className="btn btn-sm btn-danger" onClick={() => void stopSimilarity()}>
            Stop similarity
          </button>
        )}
        <button
          className={`btn btn-sm${selectionMode ? " btn-primary" : ""}`}
          onClick={() => store().setSelectionMode(!selectionMode)}
          disabled={anyPassRunning || operating || findingSimilar || photos.total === 0}
          title="Cull the library: mark photographs to keep or reject, then rename, move, copy or trash them."
        >
          {selectionMode ? "Done culling" : "Cull"}
        </button>
        {sessionId !== null && (
          <button
            className="btn btn-sm btn-primary"
            onClick={() => setReviewMode(true)}
            disabled={anyPassRunning || operating || findingSimilar || photos.total === 0}
            title="Review this shoot in capture-time order, with burst and similar-frame context. No files are changed."
          >
            Review this shoot
          </button>
        )}
      </div>

      {group === null ? (
        <>
          <FilterBar
            draft={filterConditions}
            onChange={(c) => store().setFilterConditions(c)}
            disabled={anyPassRunning}
            sessionId={sessionId}
          />

          {sessionId !== null && (
            <div className="review-presets" aria-label="Review views">
              <span className="faint">Review views</span>
              <button className="btn btn-sm" onClick={() => store().setFilterConditions([{ field: "review_state", operator: "is-null", value: null }])}>Unreviewed</button>
              <button className="btn btn-sm" onClick={() => store().setFilterConditions([{ field: "review_state", operator: "=", value: "selected" }])}>Kept</button>
              <button className="btn btn-sm" onClick={() => store().setFilterConditions([{ field: "review_state", operator: "=", value: "needs_attention" }])}>Needs attention</button>
            </div>
          )}

          {filterConditions.length > 0 && (
            <div className="library-summaryline filter-saveline">
              <span className="faint" style={{ fontSize: 12 }}>
                Current filter:
              </span>
              <button
                className={`btn btn-sm${saveViewOpen ? " btn-primary" : ""}`}
                onClick={() => setSaveViewOpen(!saveViewOpen)}
                title="Save this filter with a name; it stays dynamic as the library changes."
              >
                {saveViewOpen ? "Name it…" : "Save as view"}
              </button>
              {saveViewOpen && (
                <>
                  <input
                    className="input"
                    style={{ width: 240 }}
                    placeholder="View name…"
                    value={viewName}
                    autoFocus
                    onChange={(e) => setViewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void saveCurrentView();
                      if (e.key === "Escape") setSaveViewOpen(false);
                    }}
                    aria-label="Saved view name"
                  />
                  <button
                    className="btn btn-sm btn-primary"
                    onClick={() => void saveCurrentView()}
                    disabled={busy}
                  >
                    Save
                  </button>
                </>
              )}
            </div>
          )}
        </>
      ) : (
        <div className="library-summaryline groupbackbar">
          <button className="btn btn-sm" onClick={() => { setGroup(null); setViewerId(null); }}>
            ← Back to library
          </button>
          <span style={{ fontWeight: 600 }}>{groupLabel(group.group_type, group.photo_count)}</span>
          <span className="faint" style={{ fontSize: 12 }}>
            {group.group_type === "burst"
              ? "photographs captured within seconds of each other"
              : group.group_type === "face"
                ? "photographs with similar locally measured face appearance"
                : "photographs with near-identical structure"}
          </span>
        </div>
      )}

      {scanning && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} files`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {scanSummary && !scanning && (
        <div className="library-summaryline mono">
          Last scan “{scanSummary.session_name}”: {scanSummary.indexed.toLocaleString()} indexed · {scanSummary.ignored.toLocaleString()} ignored
          {scanSummary.cancelled ? " (stopped)" : ""} · {(scanSummary.elapsed_ms / 1000).toFixed(1)}s
          {scanSummary.errors.length > 0 && (
            <span style={{ color: "var(--warning)" }}> · {scanSummary.errors.length} error{scanSummary.errors.length > 1 ? "s" : ""} in log</span>
          )}
        </div>
      )}

      {analysisSummary && !analyzing && (
        <div className="library-summaryline mono">
          Last analysis: {analysisSummary.analyzed.toLocaleString()} measured
          {analysisSummary.failed > 0 && (
            <span style={{ color: "var(--warning)" }}> · {analysisSummary.failed.toLocaleString()} failed</span>
          )}
          · {(analysisSummary.elapsed_ms / 1000).toFixed(1)}s
          {analysisSummary.cancelled ? " (stopped)" : ""}
        </div>
      )}

      {analyzing && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} photographs`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {readingMetadata && progress && (
        <div className="library-scanline">
          <ProgressBar
            value={progress.done}
            max={progress.total}
            label={
              progress.total > 0
                ? `${progress.done.toLocaleString()} / ${progress.total.toLocaleString()} photographs — ${progress.stage}`
                : progress.stage
            }
          />
          {progress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {progress.current}
            </div>
          )}
        </div>
      )}

      {metadataSummary && !readingMetadata ? (
        <div className="library-summaryline mono">
          Last metadata read: {metadataSummary.processed.toLocaleString()} photographs
          {metadataSummary.failed > 0 && (
            <span style={{ color: "var(--warning)" }}>
              {" "}· {metadataSummary.failed.toLocaleString()} unreadable
            </span>
          )}
          {" "}· {(metadataSummary.elapsed_ms / 1000).toFixed(1)}s
          {metadataSummary.cancelled ? " (stopped)" : ""}
        </div>
      ) : null}

      {findingSimilar && similarityProgress && (
        <div className="library-scanline">
          <ProgressBar
            value={similarityProgress.done}
            max={similarityProgress.total}
            label={
              similarityProgress.total > 0
                ? `Hashing ${similarityProgress.done.toLocaleString()} / ${similarityProgress.total.toLocaleString()} photographs`
                : similarityProgress.stage
            }
          />
          {similarityProgress.current && (
            <div className="faint mono" style={{ fontSize: 11, marginTop: 4, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
              {similarityProgress.current}
            </div>
          )}
        </div>
      )}

      {!findingSimilar && similaritySummary && group === null && (
        <div className="library-summaryline mono">
          Similarity: {similaritySummary.similar_groups.toLocaleString()} similar group{similaritySummary.similar_groups === 1 ? "" : "s"} ·{" "}
          {similaritySummary.face_groups > 0 && <>{similaritySummary.face_groups.toLocaleString()} face-appearance group{similaritySummary.face_groups === 1 ? "" : "s"} ·{" "}</>}
          {similaritySummary.burst_groups.toLocaleString()} burst{similaritySummary.burst_groups === 1 ? "" : "s"}
          {similaritySummary.hashed > 0 ? ` · ${similaritySummary.hashed.toLocaleString()} hashed in this run` : ""}
          {similaritySummary.failed > 0 && (
            <span style={{ color: "var(--warning)" }}> · {similaritySummary.failed.toLocaleString()} unreadable</span>
          )}
          {" "}· {(similaritySummary.elapsed_ms / 1000).toFixed(1)}s
          {similaritySummary.cancelled ? " (stopped)" : ""}
        </div>
      )}

      {group === null && !findingSimilar && (similarityGroups?.length ?? 0) > 0 && (() => {
        const similarOnly = similarityGroups!.filter((g) => g.group_type === "similar");
        const faceOnly = similarityGroups!.filter((g) => g.group_type === "face");
        const burstOnly = similarityGroups!.filter((g) => g.group_type === "burst");
        const filtered = groupTab === "similar" ? similarOnly : groupTab === "face" ? faceOnly : groupTab === "burst" ? burstOnly : similarityGroups!;
        const visible = filtered.slice(0, groupsVisible);
        return (
        <div className="similars">
          <div className="similars-head">
            <span style={{ fontWeight: 600 }}>
              {groupTab === "all" ? `Groups — ${similarOnly.length} similar · ${faceOnly.length} face appearance · ${burstOnly.length} bursts` :
               groupTab === "similar" ? `Similar — ${similarOnly.length} groups` :
               groupTab === "face" ? `Face appearance — ${faceOnly.length} groups` :
               `Bursts — ${burstOnly.length} groups`}
            </span>
            <span className="faint" style={{ fontSize: 12 }}>
              {groupTab === "burst"
                ? "Runs captured within seconds — time, not look"
                : groupTab === "similar"
                ? "Near-identical structure (same moment, tight threshold)"
                : groupTab === "face"
                ? "Repeat face appearance candidates from optional local face detection — review, not identity labels"
                : "Near-duplicates, face-appearance candidates and same-moment runs, found on this machine"}
            </span>
          </div>
          <div style={{ display: "flex", gap: 6, margin: "8px 0" }}>
            {(["all", "similar", "face", "burst"] as const).map((t) => (
              <button key={t} className={`btn btn-sm${groupTab === t ? " btn-primary" : ""}`} onClick={() => setGroupTab(t)}>
                {t === "all" ? "All" : t === "similar" ? `Similar (${similarOnly.length})` : t === "face" ? `Faces (${faceOnly.length})` : `Bursts (${burstOnly.length})`}
              </button>
            ))}
          </div>
          <div className="similars-cards">
            {visible.map((g) => (
              <button
                key={g.id}
                className="group-card"
                onClick={() => void openGroup(g)}
                title={
                  g.group_type === "burst"
                    ? "A run of photographs captured within seconds of each other."
                    : g.group_type === "face"
                    ? "Candidate photographs with similar locally measured face appearance. This is not an identity label."
                    : "Photographs with near-identical structure (likely the same moment)."
                }
              >
                <span className="group-label">
                  {groupLabel(g.group_type, g.photo_count)}
                  {g.session_count >= 2 && (
                    <span className="chip" title={`Matches across ${g.session_count} imported sessions/shoots`}>
                      {g.session_count} sessions
                    </span>
                  )}
                </span>
                <span className="cover-strip">
                  {g.cover_photos.slice(0, 3).map((id) => (
                    <CoverThumb key={id} photoId={id} alt={`${g.group_type} group cover`} />
                  ))}
                  {g.photo_count > g.cover_photos.length && (
                    <span className="cover-more">+{g.photo_count - g.cover_photos.length}</span>
                  )}
                </span>
              </button>
            ))}
            {filtered.length > groupsVisible && (
              <button className="btn btn-sm" onClick={() => setGroupsVisible((v) => v + GROUPS_PAGE)}>
                Load {Math.min(GROUPS_PAGE, filtered.length - groupsVisible)} more — {filtered.length - groupsVisible} remaining
              </button>
            )}
          </div>
        </div>
        ); })()}

      {selectionMode && hasPhotos && (
        <div className="cullbar">
          <span className="faint" style={{ fontSize: 12 }}>
            Cull helpers — filters are the science; selection is the decision.
          </span>
          <button
            className="btn btn-sm"
            onClick={() => store().setFilterConditions([...filterConditions, { field: "sharpness", operator: ">=", value: 60 }])}
            title="Show sharp photographs (measured sharpness ≥ 60)"
          >
            Sharp
          </button>
          <button
            className="btn btn-sm"
            onClick={() => store().setFilterConditions([...filterConditions, { field: "scene_group", operator: "=", value: "nature" }])}
            title="Filter by the trained scene tag"
          >
            Nature
          </button>
          <span>
            {selectedIds.length.toLocaleString()} keep{selectedIds.length === 1 ? "" : "s"} · {rejectedCount.toLocaleString()} reject{rejectedCount === 1 ? "" : "ed"}
          </span>
          {selectedIds.length > 0 && (collections?.length ?? 0) > 0 && (
            <AddToCollection collections={collections!} onAdd={addToCollection} />
          )}
          <span className="spacer" />
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => store().setSelectionsBulk(pageIds, "selected")}
            disabled={operating || pageIds.length === 0}
            title="Mark every photograph on this page to keep"
          >
            Keep all shown
          </button>
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => store().setSelectionsBulk(pageIds, null)}
            disabled={operating || pageIds.length === 0}
          >
            Clear shown
          </button>
        </div>
      )}

      {selectionMode && selectedIds.length > 0 && (
        <MarksPanel photoIds={selectedIds} onApplied={() => photos.reload()} />
      )}

      {selectionMode && selectedIds.length > 0 && (
        <div className="cullbar">
          <ExportSheetButton photoIds={selectedIds} />
        </div>
      )}

      {selectionMode && selectedIds.length > 0 && (
        <FileOpsPanel photoIds={selectedIds} />
      )}

      <ErrorBanner message={error ?? (group === null ? photos.error : null)} />

      {!libraryHasPhotos ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="Nothing indexed yet">
            <p>
              {scanning ? (
                "Scanning in progress — watch the progress bar above."
              ) : scanSummary ? (
                scanSummary.indexed === 0
                  ? "The scan finished, but found no supported photos in this folder. JPG, PNG, WebP and TIFF are decoded; RAW previews are rendered locally where the camera format is supported; HEIC files are indexed without a local preview. You can point at another folder with “Open folder”."
                  : "This folder no longer has indexed photographs — they may have been trashed or moved. Re-scan to pick up anything new."
              ) : (
                "Press “Scan folder” to index every supported photo in this folder (JPG, PNG, WebP, TIFF, RAW, HEIC). Re-scans are safe: nothing is ever duplicated."
              )}
            </p>
            {!scanning && (
              <button className="btn btn-primary btn-sm" onClick={openFolder}>
                Open folder
              </button>
            )}
          </EmptyState>
        </div>
      ) : !hasPhotos && group === null && filterConditions.length > 0 ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="No photographs match these filters">
            <p>
              {filterConditions.length} condition{filterConditions.length > 1 ? "s" : ""} selected ·{" "}
              {(dbStatus?.photo_count ?? 0).toLocaleString()} photographs in the library.
            </p>
            <button className="btn btn-sm" onClick={() => store().setFilterConditions([])}>
              Clear filters
            </button>
          </EmptyState>
        </div>
      ) : !hasPhotos && group !== null && !groupLoading ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", justifyContent: "center" }}>
          <EmptyState glyph="◫" title="This group is empty">
            <p>Its photographs may have been trashed or moved. Go back and re-run the pass.</p>
            <button className="btn btn-sm" onClick={() => setGroup(null)}>Back to library</button>
          </EmptyState>
        </div>
      ) : (
        <>
          {photos.loading && photos.photos.length === 0 && group === null ? (
            <div className="library-loading">Loading library…</div>
          ) : (
            <div className="library-grid-area">
              <VirtualGrid
                itemCount={pagePhotos.length}
                onReachEnd={() => { if (group === null && photos.hasMore && !photos.loading) photos.loadMore(); }}
                render={(i) => (
                  <PhotoTile
                    photo={pagePhotos[i]}
                    onOpen={setViewerId}
                    selectionMode={selectionMode}
                    selection={selectionMode ? selections[pagePhotos[i].id] ?? null : null}
                    onKeep={keep}
                    onReject={reject}
                    onClear={clearSel}
                  />
                )}
              />
            </div>
          )}

          <div className="library-statusbar">
            {group !== null ? (
              <span>
                {groupLabel(group.group_type, group.photo_count)} · showing{" "}
                {groupPhotos.length.toLocaleString()} of {group.photo_count.toLocaleString()}
              </span>
            ) : filterConditions.length > 0 ? (
              <span>
                Showing {pagePhotos.length.toLocaleString()} of {photos.total.toLocaleString()} filtered
                {photos.hasMore ? " — scroll for more" : ""} · {filterConditions.length} filter{filterConditions.length > 1 ? "s" : ""}
              </span>
            ) : (
              <span>
                {pagePhotos.length.toLocaleString()} of {photos.total.toLocaleString()} photographs
                {photos.hasMore ? " — scroll for more" : ""}
              </span>
            )}
            {group === null && <span className="faint">Page {photos.page + 1}</span>}
            {dbStatus && dbStatus.analyzed_count > 0 && (
              <span className="faint">{dbStatus.analyzed_count.toLocaleString()} analyzed</span>
            )}
            {group === null && metadataPending > 0 && (
              <span className="faint">{metadataPending.toLocaleString()} awaiting metadata</span>
            )}
            <span className="spacer" />
            <span className="faint">Local-only index · thumbnails &amp; analysis on this machine</span>
            {group === null && (
              <button className="btn btn-ghost btn-sm" onClick={photos.reload} disabled={photos.loading}>
                Refresh
              </button>
            )}
          </div>
        </>
      )}

      {viewerId !== null && (
        <Viewer
          photoId={viewerId}
          ordered={pagePhotos}
          onClose={() => setViewerId(null)}
          onNavigate={setViewerId}
        />
      )}
    </div>
  );
}

/** Culling-bar widget: add the marked photographs to a chosen collection. */
function AddToCollection({
  collections,
  onAdd,
}: {
  collections: { id: number; name: string }[];
  onAdd: (collectionId: number) => void;
}) {
  const [chosen, setChosen] = useState<number | null>(null);
  return (
    <span className="addcol" style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
      <select
        className="input"
        style={{ width: 170 }}
        value={chosen ?? ""}
        onChange={(e) => setChosen(e.target.value === "" ? null : Number(e.target.value))}
        aria-label="Collection"
      >
        <option value="" disabled>
          Add to collection…
        </option>
        {collections.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </select>
      <button
        className="btn btn-sm"
        disabled={chosen === null}
        onClick={() => {
          if (chosen !== null) onAdd(chosen);
        }}
        title="Add every marked photograph to the collection (files are not touched)"
      >
        Add
      </button>
    </span>
  );
}

function ErrorBanner({ message }: { message: string | null }) {
  if (!message) return null;
  return (
    <div
      role="alert"
      style={{
        margin: "0",
        padding: "7px 32px",
        background: "var(--danger-soft)",
        color: "var(--danger)",
        fontSize: 12.5,
      }}
    >
      {message}
    </div>
  );
}
