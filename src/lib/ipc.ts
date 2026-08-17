/**
 * Typed IPC client. All backend access funnels through here so views never
 * touch `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  DbStatus,
  FileOpPlan,
  FileOpRow,
  PhotoFull,
  PhotoPage,
  PeriodStats,
  SelectionRow,
  SessionMetrics,
  SessionSummary,
  SessionRow,
  ThumbData,
  ThumbKind,
} from "@/types/api";

export const api = {
  appInfo: (): Promise<AppInfo> => invoke("app_info"),
  appPaths: () =>
    invoke<{ data_dir: string; cache_dir: string; log_dir: string; db_path: string; thumbnails_dir: string }>(
      "app_paths",
    ),
  dbStatus: (): Promise<DbStatus> => invoke("db_status"),
  pickFolder: (): Promise<string | null> => invoke("pick_folder"),
  setActiveFolder: (path: string): Promise<void> =>
    invoke("set_active_folder", { path }),
  getActiveFolder: (): Promise<string | null> => invoke("get_active_folder"),
  startScan: (path: string): Promise<void> => invoke("start_scan", { path }),
  stopScan: (): Promise<boolean> => invoke("stop_scan"),
  startAnalysis: (): Promise<void> => invoke("start_analysis"),
  stopAnalysis: (): Promise<boolean> => invoke("stop_analysis"),
  startMetadata: (): Promise<void> => invoke("start_metadata"),
  stopMetadata: (): Promise<boolean> => invoke("stop_metadata"),
  listPhotos: (offset: number, limit: number): Promise<PhotoPage> =>
    invoke("list_photos", { offset, limit }),
  // `filterJson` is the exact structured-filter object ("" = no filter).
  listFilteredPhotos: (
    filterJson: string,
    offset: number,
    limit: number,
  ): Promise<PhotoPage> => invoke("list_filtered_photos", { filterJson, offset, limit }),
  getPhotoFull: (id: number): Promise<PhotoFull> =>
    invoke("get_photo_full", { id }),
  getThumbnail: (photoId: number, kind: ThumbKind): Promise<ThumbData> =>
    invoke("get_thumbnail", { photoId, kind }),
  listSessions: (): Promise<SessionRow[]> => invoke("list_sessions"),
  // Statistics engine (Sprint 6). Synchronous, local aggregation.
  periodStats: (periodJson: string): Promise<PeriodStats> =>
    invoke("period_stats", { periodJson }),
  sessionSummary: (sessionId: number): Promise<SessionSummary> =>
    invoke("session_summary", { sessionId }),
  compareSessions: (sessionIds: number[]): Promise<SessionMetrics[]> =>
    invoke("compare_sessions", { sessionIds }),

  // File operations (Sprint 7). Plan = cheap synchronous preview; start =
  // background execution streaming operation-progress / operation-complete.
  planGroupRename: (
    photoIds: number[],
    template: string,
    groupName: string,
  ): Promise<FileOpPlan> =>
    invoke("plan_group_rename", { photoIds, template, groupName }),
  startGroupRename: (
    photoIds: number[],
    template: string,
    groupName: string,
  ): Promise<void> =>
    invoke("start_group_rename", { photoIds, template, groupName }),
  planMoveCopy: (
    photoIds: number[],
    destDir: string,
    op: "move" | "copy",
    onCollision: "skip" | "avoid-by-renaming",
  ): Promise<FileOpPlan> =>
    invoke("plan_move_copy", { photoIds, destDir, op, onCollision }),
  startMoveCopy: (
    photoIds: number[],
    destDir: string,
    op: "move" | "copy",
    onCollision: "skip" | "avoid-by-renaming",
  ): Promise<void> =>
    invoke("start_move_copy", { photoIds, destDir, op, onCollision }),
  planTrash: (photoIds: number[]): Promise<FileOpPlan> =>
    invoke("plan_trash", { photoIds }),
  startTrash: (photoIds: number[]): Promise<void> =>
    invoke("start_trash", { photoIds }),
  stopOperation: (): Promise<boolean> => invoke("stop_operation"),

  // Selection (culling) state.
  setSelection: (photoId: number, selection: "selected" | "rejected"): Promise<void> =>
    invoke("set_selection", { photoId, selection }),
  setSelections: (
    photoIds: number[],
    selection: "selected" | "rejected",
  ): Promise<number> => invoke("set_selections", { photoIds, selection }),
  clearSelection: (photoId: number): Promise<void> =>
    invoke("clear_selection", { photoId }),
  clearSelections: (photoIds: number[]): Promise<number> =>
    invoke("clear_selections", { photoIds }),
  listSelections: (): Promise<SelectionRow[]> => invoke("list_selections"),
  recentFileOps: (limit: number): Promise<FileOpRow[]> =>
    invoke("recent_file_ops", { limit }),
};

export type { UnlistenFn };

export function onProgress<T>(
  event: string,
  handler: (p: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (e) => handler(e.payload));
}

export class IpcError extends Error {
  constructor(message: string, readonly raw: unknown) {
    super(message);
    this.name = "IpcError";
  }
}

/** Convert Tauri IPC errors (string or object) into a friendly message. */
export function toErrorMessage(e: unknown): string {
  if (e instanceof IpcError) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const any = e as Record<string, unknown>;
    if (typeof any.message === "string") return any.message;
    if (typeof any.error === "string") return any.error;
  }
  return "Something went wrong. Check the log for details.";
}
