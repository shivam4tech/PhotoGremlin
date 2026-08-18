import { create } from "zustand";
import { api, toErrorMessage } from "@/lib/ipc";
import {
  applyTheme,
  persistTheme,
  readStoredTheme,
  type Theme,
} from "@/lib/theme";
import type {
  AiStatus,
  AnalysisSummary,
  AppInfo,
  Collection,
  DbStatus,
  FileOpRow,
  FaceSummary,
  FilterCondition,
  MetadataSummary,
  OperationSummary,
  PathsInfo,
  ProgressPayload,
  SavedView,
  ScanSummary,
  SimilarityGroup,
  SimilaritySummary,
  ViewId,
} from "@/types/api";

export type SelectionState = "selected" | "rejected";

interface AppState {
  view: ViewId;
  appInfo: AppInfo | null;
  paths: PathsInfo | null;
  dbStatus: DbStatus | null;
  activeFolder: string | null;
  /** Appearance preference ("dark" | "light"); persisted per machine. */
  theme: Theme;
  scanning: boolean;
  analyzing: boolean;
  readingMetadata: boolean;
  /// A rename/move/copy/trash operation is running.
  operating: boolean;
  progress: ProgressPayload | null;
  opProgress: ProgressPayload | null;
  opSummary: OperationSummary | null;
  recentOps: FileOpRow[];
  /// Bumped whenever files on disk change via operations, so the grid refetches.
  libraryVersion: number;
  scanSummary: ScanSummary | null;
  analysisSummary: AnalysisSummary | null;
  metadataSummary: MetadataSummary | null;
  /// Culling: photo id → selection state (only photos with a state).
  selections: Record<number, SelectionState>;
  selectionMode: boolean;
  notice: string | null;
  error: string | null;

  /**
   * The library filter as structured conditions. Lives in the store (not
   * in LibraryView) because saved views and session detail apply it from
   * other views and navigate in.
   */
  filterConditions: FilterCondition[];
  /** Saved views (null = not loaded yet). */
  savedViews: SavedView[] | null;
  /** Collections (null = not loaded yet). */
  collections: Collection[] | null;
  /** Similarity pass in flight. */
  findingSimilar: boolean;
  similarityProgress: ProgressPayload | null;
  similaritySummary: SimilaritySummary | null;
  /** Similar + burst groups (null = not loaded yet). */
  similarityGroups: SimilarityGroup[] | null;
  /**
   * Local intelligence (Sprint 9): the stored preference. AI is off by
   * default; when on, face detection auto-runs after each scan (and the
   * user can always run it on demand from Settings).
   */
  aiEnabled: boolean;
  /** ai_status result (null = not loaded yet). */
  aiStatus: AiStatus | null;
  /** Face-detection pass in flight. */
  detectingFaces: boolean;
  facesProgress: ProgressPayload | null;
  facesSummary: FaceSummary | null;

  setView: (v: ViewId) => void;
  setAppInfo: (i: AppInfo) => void;
  setPaths: (p: PathsInfo) => void;
  setDbStatus: (s: DbStatus) => void;
  setActiveFolder: (p: string | null) => void;
  /** Switch appearance, applied to the document and remembered. */
  setTheme: (t: Theme) => void;
  setScanning: (b: boolean) => void;
  setAnalyzing: (b: boolean) => void;
  setReadingMetadata: (b: boolean) => void;
  setOperating: (b: boolean) => void;
  setOpProgress: (p: ProgressPayload | null) => void;
  setOpSummary: (s: OperationSummary | null) => void;
  setRecentOps: (ops: FileOpRow[]) => void;
  setProgress: (p: ProgressPayload | null) => void;
  setScanSummary: (s: ScanSummary | null) => void;
  setAnalysisSummary: (s: AnalysisSummary | null) => void;
  setMetadataSummary: (s: MetadataSummary | null) => void;
  setSelectionMode: (b: boolean) => void;
  bumpLibraryVersion: () => void;
  setNotice: (n: string | null) => void;
  setError: (e: string | null) => void;

  refreshStatus: () => Promise<void>;
  /**
   * Pick a photo folder and make it active (Sprint 10: shared by the
   * Library's button and the ⌘/Ctrl+O shortcut). Returns the picked path
   * (null when the dialog was dismissed); throws on IPC failure so the
   * caller decides where the error is shown.
   */
  openFolder: () => Promise<string | null>;
  loadSelections: () => Promise<void>;
  /** Set one photo's selection (optimistic + persisted). */
  setSelection: (photoId: number, state: SelectionState | null) => void;
  /** Set many photos to the same selection (e.g. "select all on page"). */
  setSelectionsBulk: (photoIds: number[], state: SelectionState | null) => void;
  refreshRecentOps: () => Promise<void>;

  setFilterConditions: (conditions: FilterCondition[]) => void;
  loadSavedViews: () => Promise<void>;
  loadCollections: () => Promise<void>;
  setFindingSimilar: (b: boolean) => void;
  setSimilarityProgress: (p: ProgressPayload | null) => void;
  setSimilaritySummary: (s: SimilaritySummary | null) => void;
  loadSimilarityGroups: () => Promise<void>;

  loadAiStatus: () => Promise<void>;
  /** Persist the AI on/off preference (fire-and-forget, optimistic). */
  setAiEnabled: (b: boolean) => void;
  setDetectingFaces: (b: boolean) => void;
  setFacesProgress: (p: ProgressPayload | null) => void;
  setFacesSummary: (s: FaceSummary | null) => void;
}

let statusInFlight = false;

export const useAppStore = create<AppState>((set, get) => ({
  view: "library",
  appInfo: null,
  paths: null,
  dbStatus: null,
  activeFolder: null,
  theme: readStoredTheme(),
  scanning: false,
  analyzing: false,
  readingMetadata: false,
  operating: false,
  progress: null,
  opProgress: null,
  opSummary: null,
  recentOps: [],
  libraryVersion: 0,
  selections: {},
  selectionMode: false,
  scanSummary: null,
  analysisSummary: null,
  metadataSummary: null,
  notice: null,
  error: null,
  filterConditions: [],
  savedViews: null,
  collections: null,
  findingSimilar: false,
  similarityProgress: null,
  similaritySummary: null,
  similarityGroups: null,
  aiEnabled: false,
  aiStatus: null,
  detectingFaces: false,
  facesProgress: null,
  facesSummary: null,

  setView: (view) => set({ view }),
  setAppInfo: (appInfo) => set({ appInfo }),
  setPaths: (paths) => set({ paths }),
  setDbStatus: (dbStatus) => set({ dbStatus }),
  setActiveFolder: (activeFolder) => set({ activeFolder }),
  setTheme: (theme) => {
    set({ theme });
    persistTheme(theme);
    applyTheme(theme);
  },
  setScanning: (scanning) => set({ scanning }),
  setAnalyzing: (analyzing) => set({ analyzing }),
  setReadingMetadata: (readingMetadata) => set({ readingMetadata }),
  setOperating: (operating) => set({ operating }),
  setOpProgress: (opProgress) => set({ opProgress }),
  setOpSummary: (opSummary) => set({ opSummary }),
  setRecentOps: (recentOps) => set({ recentOps }),
  setSelectionMode: (selectionMode) => set({ selectionMode }),
  bumpLibraryVersion: () => set((s) => ({ libraryVersion: s.libraryVersion + 1 })),
  setProgress: (progress) => set({ progress }),
  setScanSummary: (scanSummary) => set({ scanSummary }),
  setAnalysisSummary: (analysisSummary) => set({ analysisSummary }),
  setMetadataSummary: (metadataSummary) => set({ metadataSummary }),
  setNotice: (notice) => set({ notice }),
  setError: (error) => set({ error }),

  refreshStatus: async () => {
    if (statusInFlight) return;
    statusInFlight = true;
    try {
      const [dbStatus, activeFolder] = await Promise.all([
        api.dbStatus(),
        api.getActiveFolder(),
      ]);
      // Always refresh counts; a folder the user just picked (or that the
      // backend hasn't confirmed yet) wins over an empty backend answer, so
      // opening a folder can never be clobbered by its own status refresh.
      set({ dbStatus, activeFolder: activeFolder ?? get().activeFolder });
    } catch {
      // Status is non-critical for the shell; errors surface when actions run.
    } finally {
      statusInFlight = false;
    }
  },

  openFolder: async () => {
    const picked = await api.pickFolder();
    if (!picked) return null;
    set({ activeFolder: picked, scanSummary: null, analysisSummary: null });
    // Persist so the next start restores this folder (get_active_folder
    // backs it); a persistence failure shows the banner but the folder
    // still stays active for this session.
    try {
      await api.setActiveFolder(picked);
    } catch (e) {
      set({ error: toErrorMessage(e) });
    }
    await get().refreshStatus();
    return picked;
  },

  loadSelections: async () => {
    try {
      const rows = await api.listSelections();
      const map: Record<number, SelectionState> = {};
      for (const r of rows) map[r.photo_id] = r.state;
      set({ selections: map });
    } catch {
      // Culling state is non-critical; keep whatever we have.
    }
  },

  setSelection: (photoId, state) => {
    // Optimistic local update, then persist (fire-and-forget).
    const next = { ...get().selections };
    if (state === null) delete next[photoId];
    else next[photoId] = state;
    set({ selections: next });
    const err = (m: string) => set({ error: m });
    if (state === null) api.clearSelection(photoId).catch((e) => err(toErrorMessage(e)));
    else api.setSelection(photoId, state).catch((e) => err(toErrorMessage(e)));
  },

  setSelectionsBulk: (photoIds, state) => {
    if (photoIds.length === 0) return;
    const next = { ...get().selections };
    for (const id of photoIds) {
      if (state === null) delete next[id];
      else next[id] = state;
    }
    set({ selections: next });
    const err = (m: string) => set({ error: m });
    if (state === null) api.clearSelections(photoIds).catch((e) => err(toErrorMessage(e)));
    else api.setSelections(photoIds, state).catch((e) => err(toErrorMessage(e)));
  },

  refreshRecentOps: async () => {
    try {
      set({ recentOps: await api.recentFileOps(30) });
    } catch {
      // Audit log is non-critical.
    }
  },

  setFilterConditions: (filterConditions) => set({ filterConditions }),

  loadSavedViews: async () => {
    try {
      set({ savedViews: await api.listSavedViews() });
    } catch {
      // Non-critical; the view shows its error state if it really fails.
    }
  },

  loadCollections: async () => {
    try {
      set({ collections: await api.listCollections() });
    } catch {
      // Non-critical.
    }
  },

  setFindingSimilar: (findingSimilar) => set({ findingSimilar }),
  setSimilarityProgress: (similarityProgress) => set({ similarityProgress }),
  setSimilaritySummary: (similaritySummary) => set({ similaritySummary }),

  loadSimilarityGroups: async () => {
    try {
      set({ similarityGroups: await api.listSimilarityGroups(50) });
    } catch {
      // Non-critical.
    }
  },

  loadAiStatus: async () => {
    try {
      const status = await api.aiStatus();
      set({ aiStatus: status, aiEnabled: status.enabled });
    } catch {
      // Non-critical: the Settings card falls back to "loading".
    }
  },

  setAiEnabled: (aiEnabled) => {
    set({ aiEnabled });
    const err = (m: string) => set({ error: m, aiEnabled: !aiEnabled });
    api.setAiEnabled(aiEnabled).catch((e) => err(toErrorMessage(e)));
  },

  setDetectingFaces: (detectingFaces) => set({ detectingFaces }),
  setFacesProgress: (facesProgress) => set({ facesProgress }),
  setFacesSummary: (facesSummary) => set({ facesSummary }),
}));

/** View metadata for the sidebar. */
export const VIEW_META: Record<ViewId, { label: string; description: string }> =
  {
    library: {
      label: "Library",
      description: "Browse, filter and work with your photographs.",
    },
    dashboard: {
      label: "Dashboard",
      description: "Understand how you actually shoot.",
    },
    sessions: {
      label: "Sessions",
      description: "Your shoots, compared over time.",
    },
    collections: {
      label: "Collections",
      description: "Manually curated sets of your photographs.",
    },
    "saved-views": {
      label: "Saved Views",
      description: "Dynamic filters you can return to.",
    },
    settings: {
      label: "Settings",
      description: "Application configuration.",
    },
  };
