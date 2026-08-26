/**
 * TypeScript mirrors of the Rust backend's IPC types.
 * Keep in sync with src-tauri/src.
 */

export type ViewId =
  | "home"
  | "library"
  | "dashboard"
  | "sessions"
  | "collections"
  | "saved-views"
  | "settings";

export interface RecentProject {
  path: string;
  name: string;
  parent: string;
  lastOpenedAt: string;
  photoCount: number;
}

export interface DashboardLayout {
  hidden: string[];
  order: string[];
}

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
  /** Photos not yet read by the EXIF/metadata pass. */
  metadata_pending: number;
  /** Culling state (Sprint 7): photos marked selected / rejected. */
  selected_count: number;
  rejected_count: number;
  /** Photos with a local-AI face result (Sprint 9). */
  faces_done: number;
  /** Photos with a scene-classification result (Sprint 18). */
  scenes_done: number;
  schema_version: number;
}

// --- Local intelligence (Sprint 9) -----------------------------------------

/** Local-AI status — the Settings "Local intelligence" card. */
export interface AiStatus {
  /** Stored preference (ai_enabled); AI is off by default. */
  enabled: boolean;
  /** True when the ONNX Runtime loaded on this machine. */
  runtime_available: boolean;
  /** Friendly reason when unavailable (null when available). */
  runtime_note: string | null;
  /** The shipped local model, reported as-is for transparency. */
  model: string;
  model_bytes: number;
  /** Photos with a stored face result / photos in the library. */
  faces_done: number;
  /** Scene model (Sprint 18): name and size, reported for transparency. */
  scene_model: string;
  scene_model_bytes: number;
  /** Photos with a stored scene result. */
  scenes_done: number;
  photo_count: number;
}

/** Result of one face-detection pass (carried in `faces-complete`). */
export interface FaceSummary {
  processed: number;
  with_faces: number;
  failed: number;
  cancelled: boolean;
  elapsed_ms: number;
  /** First few friendly per-file messages (the log has the full detail). */
  errors: string[];
}

/** Result of one scene-classification pass (`scenes-complete`). */
export interface SceneSummary {
  processed: number;
  failed: number;
  cancelled: boolean;
  elapsed_ms: number;
  /** First few friendly messages; the log holds the full detail. */
  errors: string[];
}

/** `scenes-complete` event payload: exactly one of the two is set. */
export interface SceneCompletePayload {
  summary: SceneSummary | null;
  error: string | null;
}

/** `faces-complete` event payload: exactly one of the two is set. */
export interface FaceCompletePayload {
  summary: FaceSummary | null;
  error: string | null;
}

// --- File operations (Sprint 7) -------------------------------------------

/** Culling state for one photograph. */
export interface SelectionRow {
  photo_id: number;
  state: "selected" | "rejected";
  updated_at: string;
}

/** One item in an operation plan (the preview). */
export interface PlanItem {
  photo_id: number;
  source: string;
  destination: string | null;
  note: string | null;
  ok: boolean;
}

/** A full operation plan — everything the UI previews before confirming. */
export interface FileOpPlan {
  op: "rename" | "move" | "copy" | "trash";
  items: PlanItem[];
  /** In-plan collision (two sources onto one name) aborts the whole plan. */
  aborted: boolean;
  /** Destination directory that does not exist yet (move/copy). */
  will_create_dir: string | null;
  /** Destructive ops (trash) require explicit confirmation. */
  destructive: boolean;
}

export interface OperationItemResult {
  source: string;
  destination: string | null;
  status: "done" | "failed" | "skipped" | "cancelled";
  detail: string | null;
}

export interface OperationSummary {
  op: string;
  total: number;
  processed: number;
  succeeded: number;
  failed: number;
  cancelled: boolean;
  elapsed_ms: number;
  items: OperationItemResult[];
}

export interface OperationCompletePayload {
  summary: OperationSummary | null;
  error: string | null;
}

/** Emitted when a contact-sheet export finishes (Sprint 14). */
export interface ContactSheetCompletePayload {
  files: string[];
  error: string | null;
  cancelled: boolean;
}

/** One row of the file-operations audit log. */
export interface FileOpRow {
  id: number;
  op_type: string;
  source_path: string;
  dest_path: string | null;
  status: string;
  detail: string | null;
  created_at: string;
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

/** Grid tile row (lightweight — full record comes via get_photo_full). */
export interface PhotoSummary {
  id: number;
  filename: string;
  extension: string;
  size_bytes: number | null;
  width: number | null;
  height: number | null;
  orientation: Orientation | null;
  capture_datetime: string | null;
  session_id: number | null;
  has_analysis: boolean;
  /** 1–5 stars (null = unrated). */
  rating: number | null;
  flag: boolean;
  /** "red" | "yellow" | "green" | "blue" | "purple" | "gray" | null. */
  color_label: string | null;
}

export interface PhotoPage {
  photos: PhotoSummary[];
  total: number;
}

/** Full photo + analysis (analysis fields NULL until the analysis pass). */
export interface PhotoFull {
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
  lens_make: string | null;
  software: string | null;
  /** Where camera/exposure/date values came from: "none" | "exif". */
  metadata_source: string;
  focal_length: number | null;
  iso: number | null;
  aperture: number | null;
  shutter_speed: number | null;
  capture_datetime: string | null;
  /** "exif" | "filename" | "mtime" — where the estimated date came from. */
  capture_datetime_source: string | null;
  gps_present: boolean;
  session_id: number | null;
  /** 1–5 stars (null = unrated). */
  rating: number | null;
  flag: boolean;
  /** "red" | "yellow" | "green" | "blue" | "purple" | "gray" | null. */
  color_label: string | null;
  indexed_at: string;
  file_mtime: string | null;
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
  /** Scene classification (Sprint 18): merged product chip + fine label. */
  scene_coarse: string | null;
  scene_fine: string | null;
  scene_conf: number | null;
  perceptual_hash: string | null;
  algorithm_version: number | null;
  analyzed_at: string | null;
}

export type ThumbKind = "grid" | "viewer";

export interface ThumbData {
  data_url: string;
  width: number;
  height: number;
  from_cache: boolean;
}

/** Payload of the `analysis-complete` event: exactly one field set. */
export interface AnalysisCompletePayload {
  summary: AnalysisSummary | null;
  error: string | null;
}

/** Result of one analysis pass (carried in `analysis-complete`). */
export interface AnalysisSummary {
  analyzed: number;
  failed: number;
  cancelled: boolean;
  elapsed_ms: number;
  errors: string[];
}

/** `metadata-complete` event payload: exactly one of the two is set. */
export interface MetadataCompletePayload {
  summary: MetadataSummary | null;
  error: string | null;
}

/** Result of one metadata (EXIF) pass (carried in `metadata-complete`). */
export interface MetadataSummary {
  processed: number;
  failed: number;
  cancelled: boolean;
  elapsed_ms: number;
  errors: string[];
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
  group_type: "similar" | "burst";
  photo_count: number;
  created_at: string;
  /** Distinct sessions spanned (≥ 2 = cross-session duplicates, Sprint 16). */
  session_count: number;
  /** Up to 4 photo ids (by id order) for a cover strip. */
  cover_photos: number[];
}

/** Result of one similarity pass (carried in `similarity-complete`). */
export interface SimilaritySummary {
  hashed: number;
  failed: number;
  similar_groups: number;
  burst_groups: number;
  elapsed_ms: number;
  cancelled: boolean;
}

/** `similarity-complete` event payload: exactly one of the two is set. */
export interface SimilarityCompletePayload {
  summary: SimilaritySummary | null;
  error: string | null;
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

// ---------------------------------------------------------------------------
// Statistics engine (Sprint 6). Mirrors src-tauri/src/statistics — `null`
// means "honest unavailable" (no inputs), never zero.
// ---------------------------------------------------------------------------

/** The one period model; sent as JSON to the engine. */
export type Period =
  | { kind: "today" }
  | { kind: "this-week" }
  | { kind: "this-month" }
  | { kind: "this-year" }
  | { kind: "all" }
  | { kind: "custom"; from: string; to: string };

export function periodJson(kind: string, from?: string, to?: string): string {
  switch (kind) {
    case "today":
      return JSON.stringify({ kind: "today" });
    case "this-week":
      return JSON.stringify({ kind: "this-week" });
    case "this-month":
      return JSON.stringify({ kind: "this-month" });
    case "this-year":
      return JSON.stringify({ kind: "this-year" });
    case "custom":
      return JSON.stringify({ kind: "custom", from, to });
    default:
      return JSON.stringify({ kind: "all" });
  }
}

export interface BinCount {
  label: string;
  count: number;
}

export interface UsageCount {
  name: string;
  photos: number;
  share: number;
  avg_sharpness: number | null;
  avg_iso: number | null;
}

export interface TrendPoint {
  /** "YYYY-MM" */
  month: string;
  photos: number;
  sessions: number;
  avg_sharpness: number | null;
  avg_iso: number | null;
  /** share of the month's analyzed photos that are color */
  color_share: number | null;
}

export interface SelectionStats {
  imported: number;
  selected: number;
  rejected: number;
  trashed: number;
  /** selected / imported (0..1); null when imported is 0 */
  kept_ratio: number | null;
}

export interface PeriodStats {
  period: string;
  photos: number;
  sessions: number;
  photos_per_session: number | null;
  /** analyzed subset size — denominator of the averages */
  analyzed: number;
  avg_sharpness: number | null;
  avg_brightness: number | null;
  avg_contrast: number | null;
  avg_saturation: number | null;
  /** shares 0..100 over analyzed photos */
  monochrome_share: number | null;
  color_share: number | null;
  /** shares over photos with AI face/smile data; null when none */
  faces_present_share: number | null;
  smiling_share: number | null;
  iso_histogram: BinCount[];
  aperture_histogram: BinCount[];
  focal_histogram: BinCount[];
  shutter_histogram: BinCount[];
  camera_usage: UsageCount[];
  lens_usage: UsageCount[];
  trend: TrendPoint[];
  /** null when no selection signal exists at all */
  selection: SelectionStats | null;
}

export interface SessionMetrics {
  id: number;
  name: string;
  photos: number;
  analyzed: number;
  avg_sharpness: number | null;
  avg_brightness: number | null;
  avg_contrast: number | null;
  avg_saturation: number | null;
  monochrome_share: number | null;
  color_share: number | null;
  avg_iso: number | null;
  avg_aperture: number | null;
  avg_shutter: number | null;
  start_time: string | null;
  end_time: string | null;
  duration_days: number | null;
}

export interface SessionSummary {
  session: SessionRow;
  duration_days: number | null;
  stats: PeriodStats;
}
