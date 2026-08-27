/**
 * Typed IPC client. All backend access funnels through here so views never
 * touch `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AiStatus,
  AppInfo,
  Collection,
  DbStatus,
  FileOpPlan,
  FileOpRow,
  PhotoFull,
  PhotoPage,
  PeriodStats,
  SavedView,
  ReviewQueue,
  SelectionRow,
  SessionMetrics,
  SessionSummary,
  SessionRow,
  SimilarityGroup,
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
  /** Records browser failures in the local application log. Best-effort only. */
  logClientError: (source: string, message: string, stack: string | null): Promise<void> =>
    invoke("log_client_error", { source, message, stack }),
  pickFolder: (): Promise<string | null> => invoke("pick_folder"),
  setActiveFolder: (path: string): Promise<void> =>
    invoke("set_active_folder", { path }),
  getActiveFolder: (): Promise<string | null> => invoke("get_active_folder"),
  getRecentProjects: async (): Promise<import("@/types/api").RecentProject[]> => {
    const raw = await invoke<unknown[]>("get_recent_projects");
    return (raw as unknown[]).map((r: unknown) => {
      if (typeof r === "string") return { path: r, name: r.split(/[\\/]/).filter(Boolean).pop() ?? r, parent: "", lastOpenedAt: new Date().toISOString(), photoCount: 0 };
      const o = r as Record<string, unknown>;
      return {
        path: String(o.path ?? o["photo_path"] ?? ""),
        name: String(o.name ?? String(o.path ?? "").split(/[\\/]/).filter(Boolean).pop() ?? ""),
        parent: String(o.parent ?? o["parent_path"] ?? ""),
        lastOpenedAt: String(o.lastOpenedAt ?? o["last_opened_at"] ?? new Date().toISOString()),
        photoCount: Number(o.photoCount ?? o["photo_count"] ?? 0),
      };
    });
  },
  removeRecentProject: (path: string): Promise<void> =>
    invoke("remove_recent_project", { path }),
  clearRecentProjects: (): Promise<void> => invoke("clear_recent_projects"),
  closeProject: (): Promise<void> => invoke("close_project"),
  openProject: (path: string): Promise<void> => invoke("open_project", { path }),
  getDashboardLayout: (): Promise<import("@/types/api").DashboardLayout | null> =>
    invoke("get_dashboard_layout"),
  setDashboardLayout: (layout: import("@/types/api").DashboardLayout): Promise<void> =>
    invoke("set_dashboard_layout", { layout }),
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
  reviewQueue: (sessionId: number): Promise<ReviewQueue> =>
    invoke("review_queue", { sessionId }),
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
  setSelection: (photoId: number, selection: "selected" | "rejected" | "needs_attention"): Promise<void> =>
    invoke("set_selection", { photoId, selection }),
  setSelections: (
    photoIds: number[],
    selection: "selected" | "rejected" | "needs_attention",
  ): Promise<number> => invoke("set_selections", { photoIds, selection }),
  clearSelection: (photoId: number): Promise<void> =>
    invoke("clear_selection", { photoId }),
  clearSelections: (photoIds: number[]): Promise<number> =>
    invoke("clear_selections", { photoIds }),
  listSelections: (): Promise<SelectionRow[]> => invoke("list_selections"),
  recentFileOps: (limit: number): Promise<FileOpRow[]> =>
    invoke("recent_file_ops", { limit }),

  // Contact sheets (Sprint 14) — PNG pages written into an existing folder.
  exportContactSheet: (
    destDir: string,
    title: string,
    photoIds: number[],
  ): Promise<void> => invoke("export_contact_sheet", { destDir, title, photoIds }),
  stopExport: (): Promise<boolean> => invoke("stop_export"),

  // Curatorial marks (Sprint 13). `null` leaves a mark untouched; rating 0
  // / empty color clear.
  updateMarks: (
    photoIds: number[],
    rating: number | null,
    flag: boolean | null,
    color: string | null,
  ): Promise<number> => invoke("update_marks", { photoIds, rating, flag, color }),

  // Saved views (Sprint 8) — named, dynamic filters.
  listSavedViews: (): Promise<SavedView[]> => invoke("list_saved_views"),
  // `filterJson` is validated by the grid's own engine before persisting.
  saveView: (
    name: string,
    filterJson: string,
    description: string | null,
  ): Promise<number> => invoke("save_view", { name, filterJson, description }),
  renameSavedView: (id: number, name: string): Promise<void> =>
    invoke("rename_saved_view", { id, name }),
  deleteSavedView: (id: number): Promise<void> =>
    invoke("delete_saved_view", { id }),
  /** How many photographs a saved view matches right now (dynamic). */
  savedViewCount: (id: number): Promise<number> =>
    invoke("saved_view_count", { id }),

  // Collections (Sprint 8) — curated sets; files are never touched.
  listCollections: (): Promise<Collection[]> => invoke("list_collections"),
  createCollection: (name: string, description: string | null): Promise<number> =>
    invoke("create_collection", { name, description }),
  renameCollection: (id: number, name: string): Promise<void> =>
    invoke("rename_collection", { id, name }),
  deleteCollection: (id: number): Promise<void> =>
    invoke("delete_collection", { id }),
  addToCollection: (collectionId: number, photoIds: number[]): Promise<number> =>
    invoke("add_to_collection", { collectionId, photoIds }),
  removeFromCollection: (collectionId: number, photoIds: number[]): Promise<number> =>
    invoke("remove_from_collection", { collectionId, photoIds }),
  collectionPhotos: (
    collectionId: number,
    offset: number,
    limit: number,
  ): Promise<PhotoPage> =>
    invoke("collection_photos", { collectionId, offset, limit }),

  // Similarity (Sprint 8) — background hash + group pass.
  startSimilarity: (): Promise<void> => invoke("start_similarity"),
  stopSimilarity: (): Promise<boolean> => invoke("stop_similarity"),
  listSimilarityGroups: (limit: number): Promise<SimilarityGroup[]> =>
    invoke("list_similarity_groups", { limit }),
  groupPhotos: (
    groupId: number,
    offset: number,
    limit: number,
  ): Promise<PhotoPage> => invoke("group_photos", { groupId, offset, limit }),

  // Local intelligence (Sprint 9) — optional; the whole app works with it
  // off and without the ONNX Runtime installed.
  aiStatus: (): Promise<AiStatus> => invoke("ai_status"),
  setAiEnabled: (enabled: boolean): Promise<void> =>
    invoke("set_ai_enabled", { enabled }),
  startFaces: (): Promise<void> => invoke("start_faces"),
  stopFaces: (): Promise<boolean> => invoke("stop_faces"),
  startSceneClassification: (): Promise<void> => invoke("start_scene_classification"),
  stopSceneClassification: (): Promise<boolean> => invoke("stop_scene_classification"),
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
