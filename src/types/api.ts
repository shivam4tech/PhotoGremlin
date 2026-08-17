/**
 * TypeScript mirrors of the Rust backend's IPC types.
 * Keep in sync with src-tauri/src.
 */

export type ViewId =
  | "library"
  | "dashboard"
  | "sessions"
  | "collections"
  | "saved-views"
  | "settings";

export interface AppInfo {
  name: string;
  version: string;
  platform: string;
  privacy: string;
  offline_only: boolean;
}

export interface PathsInfo {
  data_dir: string;
  cache_dir: string;
  log_dir: string;
  db_path: string;
  thumbnails_dir: string;
}

export interface DbStatus {
  photo_count: number;
  session_count: number;
  analyzed_count: number;
  schema_version: number;
}

export type Orientation = "landscape" | "portrait" | "square";

export interface Photo {
  id: number;
  path: string;
  filename: string;
  extension: string;
  size_bytes: number | null;
  width: number | null;
  height: number | null;
  orientation: Orientation | null;
  camera_make: string | null;
  camera_model: string | null;
  lens: string | null;
  focal_length: number | null;
  iso: number | null;
  aperture: number | null;
  shutter_speed: number | null;
  capture_datetime: string | null;
  gps_present: boolean;
  session_id: number | null;
  indexed_at: string;
  file_mtime: string | null;
}

export interface Analysis {
  photo_id: number;
  sharpness: number | null;
  brightness: number | null;
  contrast: number | null;
  saturation: number | null;
  highlight_clipping: number | null;
  shadow_clipping: number | null;
  is_monochrome: boolean;
  is_dark: boolean;
  is_bright: boolean;
  face_count: number | null;
  smile_count: number | null;
  perceptual_hash: string | null;
  algorithm_version: number;
  analyzed_at: string;
}

export interface PhotoWithAnalysis extends Photo {
  analysis: Analysis | null;
}

export interface Session {
  id: number;
  name: string;
  root_path: string | null;
  start_time: string | null;
  end_time: string | null;
  photo_count: number;
  created_at: string;
}

export interface ProgressPayload {
  total: number;
  done: number;
  stage: string;
  current: string | null;
}

export interface ScanSummary {
  session_id: number;
  session_name: string;
  total_files: number;
  indexed: number;
  ignored: number;
  /** Friendly per-file problems (capped at 20). */
  errors: string[];
  cancelled: boolean;
  elapsed_ms: number;
}

/** `scan-complete` event payload: exactly one of the two is set. */
export interface ScanCompletePayload {
  summary: ScanSummary | null;
  error: string | null;
}

/**
 * Structured filter — the engine's input. `conditions` are ANDed.
 * The Rust backend parses this JSON; keep field names stable.
 */
export interface FilterCondition {
  field: string;
  operator: "=" | "!=" | ">" | ">=" | "<" | "<=" | "between" | "in" | "is-null" | "not-null";
  value: unknown;
}

export interface Filter {
  operator: "AND";
  conditions: FilterCondition[];
}

export const EMPTY_FILTER: Filter = { operator: "AND", conditions: [] };

export interface SavedView {
  id: number;
  name: string;
  filter_json: string;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface Collection {
  id: number;
  name: string;
  description: string | null;
  created_at: string;
  photo_count: number;
}

export interface SimilarityGroup {
  id: number;
  hash: string;
  group_type: string;
  photo_count: number;
  created_at: string;
}

export interface SessionRow {
  id: number;
  name: string;
  root_path: string | null;
  start_time: string | null;
  end_time: string | null;
  photo_count: number;
  created_at: string;
}
