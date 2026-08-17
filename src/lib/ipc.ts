/**
 * Typed IPC client. All backend access funnels through here so views never
 * touch `invoke` directly.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppInfo,
  DbStatus,
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
  listSessions: () =>
    invoke<
      {
        id: number;
        name: string;
        root_path: string | null;
        start_time: string | null;
        end_time: string | null;
        photo_count: number;
        created_at: string;
      }[]
    >("list_sessions"),
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
