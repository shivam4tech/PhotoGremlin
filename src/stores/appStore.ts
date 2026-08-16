import { create } from "zustand";
import { api } from "@/lib/ipc";
import type {
  AppInfo,
  DbStatus,
  PathsInfo,
  ProgressPayload,
  ViewId,
} from "@/types/api";

interface AppState {
  view: ViewId;
  appInfo: AppInfo | null;
  paths: PathsInfo | null;
  dbStatus: DbStatus | null;
  activeFolder: string | null;
  scanning: boolean;
  analyzing: boolean;
  progress: ProgressPayload | null;
  notice: string | null;
  error: string | null;

  setView: (v: ViewId) => void;
  setAppInfo: (i: AppInfo) => void;
  setPaths: (p: PathsInfo) => void;
  setDbStatus: (s: DbStatus) => void;
  setActiveFolder: (p: string | null) => void;
  setScanning: (b: boolean) => void;
  setAnalyzing: (b: boolean) => void;
  setProgress: (p: ProgressPayload | null) => void;
  setNotice: (n: string | null) => void;
  setError: (e: string | null) => void;

  refreshStatus: () => Promise<void>;
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
  progress: null,
  notice: null,
  error: null,

  setView: (view) => set({ view }),
  setAppInfo: (appInfo) => set({ appInfo }),
  setPaths: (paths) => set({ paths }),
  setDbStatus: (dbStatus) => set({ dbStatus }),
  setActiveFolder: (activeFolder) => set({ activeFolder }),
  setScanning: (scanning) => set({ scanning }),
  setAnalyzing: (analyzing) => set({ analyzing }),
  setProgress: (progress) => set({ progress }),
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
