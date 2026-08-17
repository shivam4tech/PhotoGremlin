import { create } from "zustand";
import { api, toErrorMessage } from "@/lib/ipc";
import type {
  AnalysisSummary,
  AppInfo,
  DbStatus,
  FileOpRow,
  MetadataSummary,
  OperationSummary,
  PathsInfo,
  ProgressPayload,
  ScanSummary,
  ViewId,
} from "@/types/api";

export type SelectionState = "selected" | "rejected";

interface AppState {
  view: ViewId;
  appInfo: AppInfo | null;
  paths: PathsInfo | null;
  dbStatus: DbStatus | null;
  activeFolder: string | null;
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

  setView: (v: ViewId) => void;
  setAppInfo: (i: AppInfo) => void;
  setPaths: (p: PathsInfo) => void;
  setDbStatus: (s: DbStatus) => void;
  setActiveFolder: (p: string | null) => void;
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
  loadSelections: () => Promise<void>;
  /** Set one photo's selection (optimistic + persisted). */
  setSelection: (photoId: number, state: SelectionState | null) => void;
  /** Set many photos to the same selection (e.g. "select all on page"). */
  setSelectionsBulk: (photoIds: number[], state: SelectionState | null) => void;
  refreshRecentOps: () => Promise<void>;
}

let statusInFlight = false;

export const useAppStore = create<AppState>((set, get) => ({
  view: "library",
  appInfo: null,
  paths: null,
  dbStatus: null,
  activeFolder: null,
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

  setView: (view) => set({ view }),
  setAppInfo: (appInfo) => set({ appInfo }),
  setPaths: (paths) => set({ paths }),
  setDbStatus: (dbStatus) => set({ dbStatus }),
  setActiveFolder: (activeFolder) => set({ activeFolder }),
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
      if (!get().dbStatus || !get().activeFolder) {
        set({ dbStatus, activeFolder });
      } else {
        // Always refresh counts; preserve folder if backend cleared it.
        set({ dbStatus, activeFolder: activeFolder ?? get().activeFolder });
      }
    } catch {
      // Status is non-critical for the shell; errors surface when actions run.
    } finally {
      statusInFlight = false;
    }
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
