//! Statistics engine (Sprint 6) — a UI-independent service answering
//! aggregation queries against `photos` + `analysis` (+ `selections`,
//! `file_operations` for the selection ratio). The dashboard, sessions view
//! and comparisons all call this one implementation (STATISTICS.md).
//!
//! Honest-data rules (pinned in the output types):
//! - Averages/shares come from the ANALYZED rows only; zero analyzed rows →
//!   `None` (the UI says "unavailable", never "0").
//! - Face/smile shares only render when the AI columns actually hold data.
//! - Trends only contain buckets with data — no fabricated history.
//! - The engine emits numbers + labels; judgment phrasing lives in the UI.

pub mod bins;

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Utc};
use rusqlite::{OptionalExtension, ToSql};
use serde::{Deserialize, Serialize};

use crate::database::{Db, SessionRow};
use crate::error::{AppError, AppResult};
use crate::filters::SqlParam;
use crate::statistics::bins::{
    bin_counts, APERTURE_BINS, FOCAL_LABELS, ISO_BINS, SHUTTER_BINS,
};

// --------------------------------------------------------------------------
// Periods
// --------------------------------------------------------------------------

/// The one period model (STATISTICS.md): `{"kind":"this-month"}` etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Period {
    Today,
    ThisWeek,
    ThisMonth,
    ThisYear,
    All,
    Custom {
        from: String,
        to: String,
    },
}

pub fn parse_period(json: &str) -> AppResult<Period> {
    if json.trim().is_empty() {
        return Ok(Period::All);
    }
    serde_json::from_str(json)
        .map_err(|e| AppError::validation(format!("Could not read the period: {e}")))
}

fn rfc3339_z(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Bare "YYYY-MM-DD" → first/last second of that UTC day (stored
/// timestamps are second-precision, so the bounds cover the whole day).
fn start_of_day_utc(date: &str) -> Option<String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0).map(|t| rfc3339_z(t.and_utc())))
}

fn end_of_day_utc(date: &str) -> Option<String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(23, 59, 59).map(|t| rfc3339_z(t.and_utc())))
}

/// Monday-based (ISO) week start for a date.
fn monday_of(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

/// A period resolved against `now` to a time range expressed as RFC3339
/// strings — the catalog stores times that way in UTC, so string comparison
/// IS time comparison. Photos without a capture datetime fall back to
/// `indexed_at` at query time (`COALESCE`), not here.
#[derive(Debug, Clone)]
pub struct PeriodRange {
    pub label: String,
    /// Inclusive lower bound.
    pub from: Option<String>,
    /// Upper bound: exclusive when `to_exclusive` (clean calendar edges),
    /// inclusive otherwise (custom ranges reach end-of-day).
    pub to: Option<String>,
    pub to_exclusive: bool,
}

/// Resolve a period at `now`. Pure — `now` is injected so tests pin it.
pub fn resolve_period(p: &Period, now: DateTime<Utc>) -> PeriodRange {
    let d = now.date_naive();
    let day_start = |nd: NaiveDate| rfc3339_z(nd.and_hms_opt(0, 0, 0).unwrap().and_utc());
    match p {
        Period::Today => PeriodRange {
            label: "Today".into(),
            from: Some(day_start(d)),
            to: Some(day_start(d + Duration::days(1))),
            to_exclusive: true,
        },
        Period::ThisWeek => {
            let monday = monday_of(d);
            PeriodRange {
                label: "This week".into(),
                from: Some(day_start(monday)),
                to: Some(day_start(monday + Duration::days(7))),
                to_exclusive: true,
            }
        }
        Period::ThisMonth => PeriodRange {
            label: "This month".into(),
            from: Some(day_start(
                NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap(),
            )),
            to: {
                let next = if d.month() == 12 {
                    NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
                } else {
                    NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
                };
                Some(day_start(next))
            },
            to_exclusive: true,
        },
        Period::ThisYear => PeriodRange {
            label: "This year".into(),
            from: Some(day_start(NaiveDate::from_ymd_opt(d.year(), 1, 1).unwrap())),
            to: Some(day_start(NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap())),
            to_exclusive: true,
        },
        Period::All => PeriodRange {
            label: "All time".into(),
            from: None,
            to: None,
            to_exclusive: true,
        },
        Period::Custom { from, to } => {
            let from = from.trim();
            let to = to.trim();
            PeriodRange {
                label: format!("{from} → {to}"),
                from: Some(start_of_day_utc(from).unwrap_or_else(|| from.to_string())),
                to: Some(end_of_day_utc(to).unwrap_or_else(|| to.to_string())),
                to_exclusive: false,
            }
        }
    }
}

// --------------------------------------------------------------------------
// Scopes
// --------------------------------------------------------------------------

/// Two ways to scope the engine: a time period, or one session.
#[derive(Debug, Clone)]
enum Scope {
    Period(PeriodRange),
    Session { id: i64, label: String },
}

impl Scope {
    /// (`WHERE` clause over the standard aliases `p` = photos,
    /// `a` = analysis, bound params in order). Empty string = no filter.
    fn where_clause(&self) -> (String, Vec<SqlParam>) {
        match self {
            Scope::Period(r) => {
                let t = "COALESCE(p.capture_datetime, p.indexed_at)";
                let mut sql = String::new();
                let mut params = Vec::new();
                if let Some(f) = &r.from {
                    sql.push_str(&format!("{t} >= ?"));
                    params.push(SqlParam::Text(f.clone()));
                }
                if let Some(to) = &r.to {
                    if !sql.is_empty() {
                        sql.push_str(" AND ");
                    }
                    let op = if r.to_exclusive { "<" } else { "<=" };
                    sql.push_str(&format!("{t} {op} ?"));
                    params.push(SqlParam::Text(to.clone()));
                }
                (sql, params)
            }
            Scope::Session { id, .. } => ("p.session_id = ?".into(), vec![SqlParam::Int(*id)]),
        }
    }

    fn label(&self) -> String {
        match self {
            Scope::Period(r) => r.label.clone(),
            Scope::Session { label, .. } => label.clone(),
        }
    }
}

// --------------------------------------------------------------------------
// Result types
// --------------------------------------------------------------------------

/// One usage row (camera or lens). Numbers only — the UI must not rank
/// ("best lens") per STATISTICS.md language discipline.
#[derive(Debug, Clone, Serialize)]
pub struct UsageCount {
    pub name: String,
    pub photos: u32,
    pub share: f64,
    pub avg_sharpness: Option<f64>,
    pub avg_iso: Option<f64>,
}

/// One monthly bucket; only months with data appear (no fabricated history).
#[derive(Debug, Clone, Serialize)]
pub struct TrendPoint {
    /// "YYYY-MM".
    pub month: String,
    pub photos: u32,
    pub sessions: u32,
    pub avg_sharpness: Option<f64>,
    pub avg_iso: Option<f64>,
    /// Share of the month's ANALYZED photos that are in color.
    pub color_share: Option<f64>,
}

/// Selection-ratio inputs. Present only when a selection signal exists
/// (selection state or a move/copy/rename/trash operation).
#[derive(Debug, Clone, Serialize)]
pub struct SelectionStats {
    pub imported: u32,
    pub selected: u32,
    pub rejected: u32,
    pub trashed: u32,
    /// selected / imported (0.0–1.0), only when imported > 0.
    pub kept_ratio: Option<f64>,
}

/// Everything the dashboard needs for one scope. `None` values are honest
/// "unavailable" — the UI never renders them as zero.
#[derive(Debug, Clone, Serialize)]
pub struct PeriodStats {
    pub period: String,
    pub photos: u32,
    pub sessions: u32,
    pub photos_per_session: Option<f64>,
    /// Analyzed subset size — the denominator of the averages.
    pub analyzed: u32,
    pub avg_sharpness: Option<f64>,
    pub avg_brightness: Option<f64>,
    pub avg_contrast: Option<f64>,
    pub avg_saturation: Option<f64>,
    /// Shares over ANALYZED photos (0.0–100.0).
    pub monochrome_share: Option<f64>,
    pub color_share: Option<f64>,
    /// Shares over photos holding AI face/smile data; None when no such data.
    pub faces_present_share: Option<f64>,
    pub smiling_share: Option<f64>,
    pub iso_histogram: Vec<crate::statistics::bins::BinCount>,
    pub aperture_histogram: Vec<crate::statistics::bins::BinCount>,
    pub focal_histogram: Vec<crate::statistics::bins::BinCount>,
    pub shutter_histogram: Vec<crate::statistics::bins::BinCount>,
    pub camera_usage: Vec<UsageCount>,
    pub lens_usage: Vec<UsageCount>,
    pub trend: Vec<TrendPoint>,
    pub selection: Option<SelectionStats>,
}

/// One session in a side-by-side comparison (same metric rows for all).
#[derive(Debug, Clone, Serialize)]
pub struct SessionMetrics {
    pub id: i64,
    pub name: String,
    pub photos: u32,
    pub analyzed: u32,
    pub avg_sharpness: Option<f64>,
    pub avg_brightness: Option<f64>,
    pub avg_contrast: Option<f64>,
    pub avg_saturation: Option<f64>,
    pub monochrome_share: Option<f64>,
    pub color_share: Option<f64>,
    pub avg_iso: Option<f64>,
    pub avg_aperture: Option<f64>,
    pub avg_shutter: Option<f64>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub duration_days: Option<f64>,
}

/// A session summary: all stats scoped to the session + shoot duration.
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummary {
    pub session: SessionRow,
    pub duration_days: Option<f64>,
    pub stats: PeriodStats,
}

const TREND_MAX_MONTHS: i64 = 36;
const USAGE_MAX: i64 = 20;
/// Side-by-side comparison cap.
pub const COMPARE_MAX_SESSIONS: usize = 8;

// --------------------------------------------------------------------------
// Engine
// --------------------------------------------------------------------------

/// Scope stats for a time period. `now` is injected for testability.
pub fn period_stats(db: &Db, period: &Period, now: DateTime<Utc>) -> AppResult<PeriodStats> {
    let scope = Scope::Period(resolve_period(period, now));
    stats_for_scope(db, &scope)
}

/// All stats scoped to one session (+ its duration); friendly error for an
/// unknown session id.
pub fn session_summary(db: &Db, session_id: i64) -> AppResult<SessionSummary> {
    let session = db
        .session_by_id(session_id)?
        .ok_or_else(|| AppError::validation(format!("Unknown session {session_id}")))?;
    let scope = Scope::Session {
        id: session_id,
        label: format!("Session: {}", session.name),
    };
    let stats = stats_for_scope(db, &scope)?;
    let duration_days = session_duration_days(db, session_id)?;
    Ok(SessionSummary {
        session,
        duration_days,
        stats,
    })
}

/// Side-by-side metrics for a few sessions (capped at `COMPARE_MAX_SESSIONS`).
pub fn compare_sessions(db: &Db, session_ids: Vec<i64>) -> AppResult<Vec<SessionMetrics>> {
    if session_ids.is_empty() || session_ids.len() > COMPARE_MAX_SESSIONS {
        return Err(AppError::validation(format!(
            "Compare 1 to {COMPARE_MAX_SESSIONS} sessions at a time"
        )));
    }
    let mut out = Vec::with_capacity(session_ids.len());
    for id in &session_ids {
        let session = db
            .session_by_id(*id)?
            .ok_or_else(|| AppError::validation(format!("Unknown session {id}")))?;
        let scope = Scope::Session {
            id: *id,
            label: session.name.clone(),
        };
        let s = stats_for_scope(db, &scope)?;
        out.push(SessionMetrics {
            id: *id,
            name: session.name,
            photos: s.photos,
            analyzed: s.analyzed,
            avg_sharpness: s.avg_sharpness,
            avg_brightness: s.avg_brightness,
            avg_contrast: s.avg_contrast,
            avg_saturation: s.avg_saturation,
            monochrome_share: s.monochrome_share,
            color_share: s.color_share,
            avg_iso: session_column_avg(db, *id, "p.iso", "ISO")?,
            avg_aperture: session_column_avg(db, *id, "p.aperture", "aperture")?,
            avg_shutter: session_column_avg(db, *id, "p.shutter_speed", "shutter speed")?,
            start_time: session.start_time,
            end_time: session.end_time,
            duration_days: session_duration_days(db, *id)?,
        });
    }
    Ok(out)
}

// --------------------------------------------------------------------------
// Queries
// --------------------------------------------------------------------------

/// Shoot duration in days from the photos' best-known times.
fn session_duration_days(db: &Db, session_id: i64) -> AppResult<Option<f64>> {
    let conn = db.lock()?;
    let pair: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT MIN(COALESCE(capture_datetime, indexed_at)),
                    MAX(COALESCE(capture_datetime, indexed_at))
             FROM photos WHERE session_id = ?1",
            [session_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()
        .map_err(sql_err("query session duration"))?;
    Ok(pair.and_then(|(lo, hi)| {
        let lo = lo?;
        let hi = hi?;
        let lo: DateTime<FixedOffset> = chrono::DateTime::parse_from_rfc3339(&lo).ok()?;
        let hi: DateTime<FixedOffset> = chrono::DateTime::parse_from_rfc3339(&hi).ok()?;
        Some((hi - lo).num_seconds() as f64 / 86_400.0)
    }))
}

/// AVG of one fixed column for one session (comparisons). The column is a
/// compile-time literal allow-list, never user input.
fn session_column_avg(
    db: &Db,
    session_id: i64,
    column: &'static str,
    what: &'static str,
) -> AppResult<Option<f64>> {
    debug_assert!(matches!(
        column,
        "p.iso" | "p.aperture" | "p.shutter_speed"
    ));
    let conn = db.lock()?;
    conn.query_row(
        &format!("SELECT AVG({column}) FROM photos p WHERE p.session_id = ?1"),
        [session_id],
        |r| r.get(0),
    )
    .map_err(sql_err(&format!("query session average {what}")))
}

fn sql_err(context: &str) -> impl FnOnce(rusqlite::Error) -> AppError {
    let context = context.to_string();
    move |e| {
        tracing::error!(%context, error = %e, "sqlite error");
        AppError::Database(e.to_string())
    }
}

fn where_sql_or_one(where_sql: &str) -> String {
    if where_sql.is_empty() {
        "1=1".into()
    } else {
        where_sql.to_string()
    }
}

/// All aggregation queries for one scope, each in its own short lock hold.
fn stats_for_scope(db: &Db, scope: &Scope) -> AppResult<PeriodStats> {
    let (where_sql, params) = scope.where_clause();

    let (photos, sessions): (u32, u32) = {
        let conn = db.lock()?;
        let owned = bind(&params);
        conn.query_row(
            &format!(
                "SELECT COUNT(*), COUNT(DISTINCT p.session_id)
                 FROM photos p WHERE {}",
                where_sql_or_one(&where_sql)
            ),
            owned.as_slice(),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql_err("query stats totals"))?
    };

    // Analyzed subset: count, the four 0–100 averages, monochrome tally.
    // Zero analyzed rows → AVG is SQL NULL → None (honest "unavailable").
    let (analyzed, avg_sharpness, avg_brightness, avg_contrast, avg_saturation, mono): (
        u32,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        Option<f64>,
        u32,
    ) = {
        let conn = db.lock()?;
        let owned = bind(&params);
        conn.query_row(
            &format!(
                "SELECT COUNT(*), AVG(a.sharpness), AVG(a.brightness), AVG(a.contrast),
                        AVG(a.saturation), COALESCE(SUM(a.is_monochrome), 0)
                 FROM photos p
                 JOIN analysis a ON a.photo_id = p.id
                 WHERE {}",
                where_sql_or_one(&where_sql)
            ),
            owned.as_slice(),
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, i64>(5)? as u32,
                ))
            },
        )
        .map_err(sql_err("query stats analyzed"))?
    };
    let monochrome_share = (analyzed > 0).then(|| mono as f64 * 100.0 / analyzed as f64);
    let color_share = (analyzed > 0).then(|| (analyzed - mono) as f64 * 100.0 / analyzed as f64);

    let faces_present_share = ai_share(db, &where_sql, &params, "face_count")?;
    let smiling_share = ai_share(db, &where_sql, &params, "smile_count")?;

    // EXIF column values in scope → pure binning in Rust (bins.rs).
    let (exif_iso, exif_aperture, exif_focal, exif_shutter): (
        Vec<Option<i64>>,
        Vec<Option<f64>>,
        Vec<Option<f64>>,
        Vec<Option<f64>>,
    ) = {
        let conn = db.lock()?;
        let owned = bind(&params);
        let mut stmt = conn
            .prepare(&format!(
                "SELECT p.iso, p.aperture, p.focal_length, p.shutter_speed
                 FROM photos p WHERE {}",
                where_sql_or_one(&where_sql)
            ))
            .map_err(sql_err("prepare exif columns"))?;
        let rows = stmt
            .query_map(owned.as_slice(), |r| {
                Ok((
                    r.get::<_, Option<i64>>(0)?,
                    r.get::<_, Option<f64>>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                    r.get::<_, Option<f64>>(3)?,
                ))
            })
            .map_err(sql_err("query exif columns"))?;
        let mut iso = Vec::new();
        let mut ap = Vec::new();
        let mut fo = Vec::new();
        let mut sh = Vec::new();
        for row in rows {
            let (i, a, f, s) = row.map_err(sql_err("read exif row"))?;
            iso.push(i);
            ap.push(a);
            fo.push(f);
            sh.push(s);
        }
        (iso, ap, fo, sh)
    };

    let camera_usage = usage_table(db, &where_sql, &params, "p.camera_make")?;
    let lens_usage = usage_table(db, &where_sql, &params, "p.lens")?;

    // Monthly trend within the scope: only months with data, most recent
    // capped, returned chronologically.
    let trend: Vec<TrendPoint> = {
        let conn = db.lock()?;
        let owned = bind(&params);
        let mut stmt = conn
            .prepare(&format!(
                "SELECT substr(COALESCE(p.capture_datetime, p.indexed_at), 1, 7) AS month,
                        COUNT(*) AS photos,
                        COUNT(DISTINCT p.session_id) AS sessions,
                        AVG(a.sharpness) AS avg_sharpness,
                        AVG(p.iso) AS avg_iso,
                        COUNT(a.photo_id) AS analyzed,
                        COALESCE(SUM(CASE WHEN a.is_monochrome = 1 THEN 1 ELSE 0 END), 0) AS mono
                 FROM photos p
                 LEFT JOIN analysis a ON a.photo_id = p.id
                 WHERE {}
                 GROUP BY month
                 ORDER BY month DESC
                 LIMIT {TREND_MAX_MONTHS}",
                where_sql_or_one(&where_sql)
            ))
            .map_err(sql_err("prepare trend"))?;
        let rows = stmt
            .query_map(owned.as_slice(), |r| {
                let analyzed: i64 = r.get(5)?;
                let mono: i64 = r.get(6)?;
                Ok(TrendPoint {
                    month: r.get(0)?,
                    photos: r.get(1)?,
                    sessions: r.get(2)?,
                    avg_sharpness: r.get(3)?,
                    avg_iso: r.get(4)?,
                    color_share: (analyzed > 0)
                        .then(|| (analyzed - mono) as f64 * 100.0 / analyzed as f64),
                })
            })
            .map_err(sql_err("query trend"))?;
        let mut pts = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(sql_err("read trend rows"))?;
        pts.reverse();
        pts
    };

    let selection = selection_stats(db, &where_sql, &params, photos)?;

    Ok(PeriodStats {
        period: scope.label(),
        photos,
        sessions,
        photos_per_session: (sessions > 0).then(|| photos as f64 / sessions as f64),
        analyzed,
        avg_sharpness,
        avg_brightness,
        avg_contrast,
        avg_saturation,
        monochrome_share,
        color_share,
        faces_present_share,
        smiling_share,
        iso_histogram: bin_counts(&exif_iso, bins::iso_bin, ISO_BINS),
        aperture_histogram: bin_counts(&exif_aperture, bins::aperture_bin, APERTURE_BINS),
        focal_histogram: bin_counts(&exif_focal, bins::focal_bin, FOCAL_LABELS),
        shutter_histogram: bin_counts(&exif_shutter, bins::shutter_bin, SHUTTER_BINS),
        camera_usage,
        lens_usage,
        trend,
        selection,
    })
}

/// Share of photos (in scope) that carry non-zero values for one AI column;
/// None when no photo in scope has that column at all (honest).
fn ai_share(
    db: &Db,
    where_sql: &str,
    params: &[SqlParam],
    column: &str,
) -> AppResult<Option<f64>> {
    let column = if column == "face_count" || column == "smile_count" {
        column
    } else {
        return Ok(None);
    };
    let conn = db.lock()?;
    let owned = bind(params);
    let (total, present) = conn
        .query_row(
            &format!(
                "SELECT COUNT(*),
                        COALESCE(SUM(CASE WHEN a.{column} > 0 THEN 1 ELSE 0 END), 0)
                 FROM photos p
                 JOIN analysis a ON a.photo_id = p.id
                 WHERE {} AND a.{column} IS NOT NULL",
                where_sql_or_one(where_sql)
            ),
            owned.as_slice(),
            |r| Ok((r.get::<_, u32>(0)?, r.get::<_, u32>(1)?)),
        )
        .map_err(sql_err("query AI share"))?;
    Ok((total > 0).then(|| present as f64 * 100.0 / total as f64))
}

/// Camera or lens usage: top `USAGE_MAX` by count, with share of the scope,
/// and (analyzed-only) average sharpness and average ISO. NULL/empty names
/// group as "Unknown camera" / "Unknown lens".
fn usage_table(
    db: &Db,
    where_sql: &str,
    params: &[SqlParam],
    name_expr: &str,
) -> AppResult<Vec<UsageCount>> {
    let name_expr = match name_expr {
        "p.camera_make" => "COALESCE(NULLIF(TRIM(p.camera_make), ''), 'Unknown camera')",
        "p.lens" => "COALESCE(NULLIF(TRIM(p.lens), ''), 'Unknown lens')",
        other => return Err(AppError::validation(format!("unknown usage column {other}"))),
    };
    let conn = db.lock()?;
    let owned = bind(params);
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {name_expr} AS name, COUNT(*) AS photos, AVG(a.sharpness), AVG(p.iso)
             FROM photos p
             LEFT JOIN analysis a ON a.photo_id = p.id
             WHERE {}
             GROUP BY 1
             ORDER BY photos DESC, name ASC
             LIMIT {USAGE_MAX}",
            where_sql_or_one(where_sql)
        ))
        .map_err(sql_err("prepare usage table"))?;
    let rows = stmt
        .query_map(owned.as_slice(), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
            ))
        })
        .map_err(sql_err("query usage table"))?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(sql_err("read usage rows"))?;

    let total: u32 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM photos p WHERE {}", where_sql_or_one(where_sql)),
            owned.as_slice(),
            |r| r.get(0),
        )
        .map_err(sql_err("query usage total"))?;

    Ok(rows
        .into_iter()
        .map(|(name, photos, avg_sharpness, avg_iso)| UsageCount {
            name,
            photos,
            share: (total > 0).then(|| photos as f64 * 100.0 / total as f64).unwrap_or(0.0),
            avg_sharpness,
            avg_iso,
        })
        .collect())
}

/// Selection-ratio inputs, present only when SOME selection signal exists:
/// selection state within the scope, or a move/copy/rename/trash operation
/// anywhere (file ops pre-date the scope concept).
fn selection_stats(
    db: &Db,
    where_sql: &str,
    params: &[SqlParam],
    imported: u32,
) -> AppResult<Option<SelectionStats>> {
    let count_selections = |conn: &rusqlite::Connection, state: &str| -> AppResult<u32> {
        let mut ps: Vec<Box<dyn ToSql>> = Vec::new();
        let state_param: Box<dyn ToSql> = Box::new(SqlParam::Text(state.to_string()));
        ps.push(state_param);
        for p in params {
            let b: Box<dyn ToSql> = Box::new(p.clone());
            ps.push(b);
        }
        let ref_owned: Vec<&dyn ToSql> = ps.iter().map(|b| b.as_ref()).collect();
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM selections s
                 JOIN photos p ON p.id = s.photo_id
                 WHERE s.state = ?1 AND {}",
                where_sql_or_one(where_sql)
            ),
            ref_owned.as_slice(),
            |r| r.get(0),
        )
        .map_err(sql_err("query selection count"))
    };

    let conn = db.lock()?;
    let selected = count_selections(&conn, "selected")?;
    let rejected = count_selections(&conn, "rejected")?;
    let trashed: u32 = conn
        .query_row(
            "SELECT COUNT(DISTINCT source_path) FROM file_operations WHERE op_type = 'trash'",
            [],
            |r| r.get(0),
        )
        .map_err(sql_err("query trashed"))?;
    let other_ops: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_operations WHERE op_type IN ('move', 'copy', 'rename')",
            [],
            |r| r.get(0),
        )
        .map_err(sql_err("query file ops signal"))?;

    Ok((selected > 0 || rejected > 0 || trashed > 0 || other_ops > 0).then(|| SelectionStats {
        imported,
        selected,
        rejected,
        trashed,
        kept_ratio: (imported > 0).then(|| selected as f64 / imported as f64),
    }))
}

fn bind(params: &[SqlParam]) -> Vec<&dyn rusqlite::ToSql> {
    params.iter().map(|p| p as &dyn rusqlite::ToSql).collect()
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(y: i32, mo: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, 0, 0).unwrap()
    }

    #[test]
    fn today_resolves_to_midnight_boundary() {
        let r = resolve_period(&Period::Today, at(2026, 8, 17, 15));
        assert_eq!(r.label, "Today");
        assert_eq!(r.from.as_deref(), Some("2026-08-17T00:00:00Z"));
        assert_eq!(r.to.as_deref(), Some("2026-08-18T00:00:00Z"));
        assert!(r.to_exclusive);
    }

    #[test]
    fn week_starts_on_monday() {
        // 2026-08-17 is a Monday.
        let r = resolve_period(&Period::ThisWeek, at(2026, 8, 17, 9));
        assert_eq!(r.from.as_deref(), Some("2026-08-17T00:00:00Z"));
        assert_eq!(r.to.as_deref(), Some("2026-08-24T00:00:00Z"));

        // From a Sunday, the week already started 6 days earlier.
        let r = resolve_period(&Period::ThisWeek, at(2026, 8, 23, 9));
        assert_eq!(r.from.as_deref(), Some("2026-08-17T00:00:00Z"));
    }

    #[test]
    fn month_and_year_roll_year_boundary() {
        let r = resolve_period(&Period::ThisMonth, at(2026, 12, 31, 23));
        assert_eq!(r.from.as_deref(), Some("2026-12-01T00:00:00Z"));
        assert_eq!(r.to.as_deref(), Some("2027-01-01T00:00:00Z"));

        let r = resolve_period(&Period::ThisYear, at(2026, 1, 1, 0));
        assert_eq!(r.from.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(r.to.as_deref(), Some("2027-01-01T00:00:00Z"));
        assert!(r.to_exclusive);
    }

    #[test]
    fn custom_range_extends_bare_date_to_end_of_day() {
        let r = resolve_period(
            &Period::Custom {
                from: "2026-05-01".into(),
                to: "2026-05-03".into(),
            },
            at(2026, 8, 17, 0),
        );
        assert_eq!(r.from.as_deref(), Some("2026-05-01T00:00:00Z"));
        assert_eq!(r.to.as_deref(), Some("2026-05-03T23:59:59Z"));
        assert!(!r.to_exclusive);
    }

    #[test]
    fn period_json_roundtrip() {
        assert_eq!(parse_period("").unwrap(), Period::All);
        assert_eq!(
            parse_period(r#"{"kind":"this-month"}"#).unwrap(),
            Period::ThisMonth
        );
        assert_eq!(
            parse_period(r#"{"kind":"custom","from":"2026-01-01","to":"2026-01-31"}"#).unwrap(),
            Period::Custom {
                from: "2026-01-01".into(),
                to: "2026-01-31".into()
            }
        );
        assert!(parse_period(r#"{"kind":"decades"}"#).is_err());
    }
}
