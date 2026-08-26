//! Filter engine (Sprint 5) — structured data, not UI state.
//!
//! One representation (FILTER_ENGINE.md) is shared by the library grid,
//! saved views, collections, and the future statistics engine. This module
//! is Tauri- and DB-independent: it parses + validates a filter, then
//! translates it to a *parameterized* SQL fragment. Column names come only
//! from a fixed registry; every user-supplied value is a bound parameter.
//! Unknown fields, operators, or value types fail with a friendly
//! `Validation` error before any SQL runs.

use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Top-level operator. v0.1 is AND-only; composability comes from many
/// conditions. Kept as data so saved views re-serialize forward-compatibly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operator {
    #[serde(rename = "AND")]
    And,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CondOperator {
    #[serde(rename = "=")]
    Eq,
    #[serde(rename = "!=")]
    Neq,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Gte,
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Lte,
    Between,
    In,
    IsNull,
    NotNull,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterCondition {
    pub field: String,
    pub operator: CondOperator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
}

/// The wire-format filter object (exactly the JSON in FILTER_ENGINE.md and
/// `saved_views.filter_json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Filter {
    pub operator: Operator,
    pub conditions: Vec<FilterCondition>,
}

impl Filter {
    pub fn empty() -> Self {
        Self {
            operator: Operator::And,
            conditions: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.conditions.is_empty()
    }
}

/// Soft cap so a saved view cannot smuggle in an absurdly expensive WHERE.
const MAX_CONDITIONS: usize = 50;
/// Cap on `in` lists.
const MAX_IN_ITEMS: usize = 100;

/// Column value kind (drives value coercion + which operators make sense).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Real,
    Int,
    Text,
    Bool,
    /// Stored as TEXT; comparisons are lexicographic (all values are UTC
    /// RFC3339, so string order == time order).
    DateTime,
}

/// One registry entry maps a filter field to the exact SQL expression that
/// produces its value, plus the kind it is. Expressions are compile-time
/// constants — user input only ever reaches bound parameters.
#[derive(Clone, Copy)]
struct FieldDef {
    kind: Kind,
    /// SQL expression (with table alias): the value a condition compares.
    expr: &'static str,
    /// `color` is the inverse of the stored monochrome flag.
    negate_bool: bool,
}

const SHARPNESS: FieldDef = FieldDef { kind: Kind::Real, expr: "a.sharpness", negate_bool: false };
const BRIGHTNESS: FieldDef = FieldDef { kind: Kind::Real, expr: "a.brightness", negate_bool: false };
const CONTRAST: FieldDef = FieldDef { kind: Kind::Real, expr: "a.contrast", negate_bool: false };
const SATURATION: FieldDef = FieldDef { kind: Kind::Real, expr: "a.saturation", negate_bool: false };
const HIGHLIGHT_CLIPPING: FieldDef = FieldDef { kind: Kind::Real, expr: "a.highlight_clipping", negate_bool: false };
const SHADOW_CLIPPING: FieldDef = FieldDef { kind: Kind::Real, expr: "a.shadow_clipping", negate_bool: false };
const MONOCHROME: FieldDef = FieldDef { kind: Kind::Bool, expr: "a.is_monochrome", negate_bool: false };
const COLOR: FieldDef = FieldDef { kind: Kind::Bool, expr: "a.is_monochrome", negate_bool: true };
const DARK: FieldDef = FieldDef { kind: Kind::Bool, expr: "a.is_dark", negate_bool: false };
const BRIGHT: FieldDef = FieldDef { kind: Kind::Bool, expr: "a.is_bright", negate_bool: false };
const ORIENTATION: FieldDef = FieldDef { kind: Kind::Text, expr: "p.orientation", negate_bool: false };
const CAMERA_MAKE: FieldDef = FieldDef { kind: Kind::Text, expr: "p.camera_make", negate_bool: false };
const CAMERA_MODEL: FieldDef = FieldDef { kind: Kind::Text, expr: "p.camera_model", negate_bool: false };
const LENS: FieldDef = FieldDef { kind: Kind::Text, expr: "p.lens", negate_bool: false };
const ISO: FieldDef = FieldDef { kind: Kind::Int, expr: "p.iso", negate_bool: false };
const APERTURE: FieldDef = FieldDef { kind: Kind::Real, expr: "p.aperture", negate_bool: false };
const SHUTTER_SPEED: FieldDef = FieldDef { kind: Kind::Real, expr: "p.shutter_speed", negate_bool: false };
const FOCAL_LENGTH: FieldDef = FieldDef { kind: Kind::Real, expr: "p.focal_length", negate_bool: false };
const CAPTURE_DATETIME: FieldDef = FieldDef { kind: Kind::DateTime, expr: "p.capture_datetime", negate_bool: false };
const SESSION_ID: FieldDef = FieldDef { kind: Kind::Int, expr: "p.session_id", negate_bool: false };
const FACES_PRESENT: FieldDef = FieldDef { kind: Kind::Bool, expr: "(a.face_count IS NOT NULL AND a.face_count > 0)", negate_bool: false };
const FACE_COUNT: FieldDef = FieldDef { kind: Kind::Int, expr: "a.face_count", negate_bool: false };
const SMILING: FieldDef = FieldDef { kind: Kind::Bool, expr: "(a.smile_count IS NOT NULL AND a.smile_count > 0)", negate_bool: false };
const SMILE_COUNT: FieldDef = FieldDef { kind: Kind::Int, expr: "a.smile_count", negate_bool: false };
const RATING: FieldDef = FieldDef { kind: Kind::Int, expr: "p.rating", negate_bool: false };
const FLAGGED: FieldDef = FieldDef { kind: Kind::Bool, expr: "p.flag = 1", negate_bool: false };
const COLOR_LABEL: FieldDef = FieldDef { kind: Kind::Text, expr: "p.color_label", negate_bool: false };
/// Scene group (Sprint 18): the MERGED product chip stored by the scene
/// pass ("nature", "urban", "home_stay", ...). Values come from the local
/// model; NULL when the scene pass has not run.
const SCENE_GROUP: FieldDef = FieldDef { kind: Kind::Text, expr: "a.scene_coarse", negate_bool: false };

/// The field registry (FILTER_ENGINE.md): maps each filter field to
/// (expression, kind) so conditions validate before any SQL runs. Unknown
/// fields fail with a friendly error.
fn field_def(name: &str) -> Option<&'static FieldDef> {
    Some(match name {
        "sharpness" => &SHARPNESS,
        "brightness" => &BRIGHTNESS,
        "contrast" => &CONTRAST,
        "saturation" => &SATURATION,
        "highlight_clipping" => &HIGHLIGHT_CLIPPING,
        "shadow_clipping" => &SHADOW_CLIPPING,
        "monochrome" => &MONOCHROME,
        "color" => &COLOR,
        "dark" => &DARK,
        "bright" => &BRIGHT,
        "orientation" => &ORIENTATION,
        "camera_make" => &CAMERA_MAKE,
        "camera_model" => &CAMERA_MODEL,
        "lens" => &LENS,
        "iso" => &ISO,
        "aperture" => &APERTURE,
        "shutter_speed" => &SHUTTER_SPEED,
        "focal_length" => &FOCAL_LENGTH,
        "capture_datetime" => &CAPTURE_DATETIME,
        "session_id" => &SESSION_ID,
        "faces_present" => &FACES_PRESENT,
        "face_count" => &FACE_COUNT,
        "smiling" => &SMILING,
        "smile_count" => &SMILE_COUNT,
        "rating" => &RATING,
        "flagged" => &FLAGGED,
        "color_label" => &COLOR_LABEL,
        "scene_group" => &SCENE_GROUP,
        _ => return None,
    })
}

/// Parse + sanity-check a filter's JSON text (the exact stored shape).
pub fn parse_filter(json: &str) -> AppResult<Filter> {
    if json.trim().is_empty() {
        return Ok(Filter::empty());
    }
    let f: Filter =
        serde_json::from_str(json).map_err(|e| AppError::validation(format!("Could not read the filter: {e}")))?;
    if f.conditions.len() > MAX_CONDITIONS {
        return Err(AppError::validation(format!(
            "A filter can have at most {MAX_CONDITIONS} conditions"
        )));
    }
    Ok(f)
}

/// Translate a parsed filter to a SQL `WHERE` fragment (or an empty string
/// when there are no conditions) plus its bound parameters, in order.
///
/// The fragment assumes the aliases `p` (photos) and `a` (analysis,
/// LEFT JOINed on photo id) are in scope — see `Db::photos_where`.
pub fn build_where(filter: &Filter) -> AppResult<(String, Vec<SqlParam>)> {
    let mut clauses: Vec<String> = Vec::with_capacity(filter.conditions.len());
    let mut params: Vec<SqlParam> = Vec::new();

    for (i, cond) in filter.conditions.iter().enumerate() {
        let def = field_def(&cond.field).ok_or_else(|| {
            AppError::validation(format!(
                "condition {} (field `{}`): unknown filter field",
                i + 1,
                cond.field
            ))
        })?;
        validate_operator(def, cond)?;
        clauses.push(build_clause(def, cond, &mut params)?);
    }

    let where_sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    Ok((where_sql, params))
}

/// A bound parameter in a stable enum (values never touch the SQL text).
#[derive(Debug, Clone, PartialEq)]
pub enum SqlParam {
    Int(i64),
    Real(f64),
    Text(String),
    Bool(bool),
}

impl ToSql for SqlParam {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            SqlParam::Int(v) => Ok(rusqlite::types::ToSqlOutput::from(*v)),
            SqlParam::Real(v) => Ok(rusqlite::types::ToSqlOutput::from(*v)),
            SqlParam::Text(v) => Ok(rusqlite::types::ToSqlOutput::from(v.as_str())),
            SqlParam::Bool(v) => Ok(rusqlite::types::ToSqlOutput::from(i64::from(*v))),
        }
    }
}

fn validate_operator(def: &FieldDef, cond: &FilterCondition) -> AppResult<()> {
    let allowed: &[CondOperator] = match def.kind {
        Kind::Real | Kind::Int => &[
            CondOperator::Eq,
            CondOperator::Neq,
            CondOperator::Gt,
            CondOperator::Gte,
            CondOperator::Lt,
            CondOperator::Lte,
            CondOperator::Between,
            CondOperator::In,
            CondOperator::IsNull,
            CondOperator::NotNull,
        ],
        // DateTime supports order + range but not `in` in v0.1 (`between`
        // covers the real use).
        Kind::DateTime => &[
            CondOperator::Eq,
            CondOperator::Neq,
            CondOperator::Gt,
            CondOperator::Gte,
            CondOperator::Lt,
            CondOperator::Lte,
            CondOperator::Between,
            CondOperator::IsNull,
            CondOperator::NotNull,
        ],
        // Flags are derived data: true/false only. (NULL analysis rows never
        // match a flag — a photo we have not measured is not "monochrome".)
        Kind::Bool => &[CondOperator::Eq, CondOperator::Neq],
        Kind::Text => &[
            CondOperator::Eq,
            CondOperator::Neq,
            CondOperator::In,
            CondOperator::IsNull,
            CondOperator::NotNull,
        ],
    };
    if !allowed.contains(&cond.operator) {
        return Err(AppError::validation(format!(
            "field `{}` does not support the `{}` operator",
            cond.field,
            op_name(&cond.operator)
        )));
    }
    Ok(())
}

fn build_clause(def: &FieldDef, cond: &FilterCondition, params: &mut Vec<SqlParam>) -> AppResult<String> {
    let expr = def.expr;
    match cond.operator {
        CondOperator::IsNull => Ok(format!("{expr} IS NULL")),
        CondOperator::NotNull => Ok(format!("{expr} IS NOT NULL")),
        CondOperator::Between => {
            let arr = value_array(def, cond, "between", "[lo, hi]")?;
            if arr.len() != 2 {
                return Err(AppError::validation(format!(
                    "field `{}`: `between` needs exactly two values",
                    cond.field
                )));
            }
            params.push(coerce(def, &arr[0], cond)?);
            params.push(coerce(def, &arr[1], cond)?);
            Ok(format!("{expr} >= ? AND {expr} <= ?"))
        }
        CondOperator::In => {
            let arr = value_array(def, cond, "in", "a list")?;
            if arr.is_empty() || arr.len() > MAX_IN_ITEMS {
                return Err(AppError::validation(format!(
                    "field `{}`: `in` needs 1..={MAX_IN_ITEMS} items",
                    cond.field
                )));
            }
            let ph: Vec<String> = (0..arr.len()).map(|_| "?".to_string()).collect();
            for item in arr.iter() {
                params.push(coerce(def, item, cond)?);
            }
            Ok(format!("{expr} IN ({})", ph.join(", ")))
        }
        // Eq / Neq / Gt / Gte / Lt / Lte all reduce to one bound comparison.
        op => {
            let v = cond.value.as_ref().ok_or_else(|| {
                AppError::validation(format!("field `{}`: missing value", cond.field))
            })?;
            let p = coerce(def, v, cond)?;
            // `color` binds the inverse of the stored monochrome flag.
            params.push(if def.negate_bool {
                if let SqlParam::Bool(b) = p {
                    SqlParam::Bool(!b)
                } else {
                    p
                }
            } else {
                p
            });
            Ok(format!("{expr} {} ?", op_symbol(&op)))
        }
    }
}

fn value_array(
    def: &FieldDef,
    cond: &FilterCondition,
    op: &str,
    expected: &str,
) -> AppResult<Vec<serde_json::Value>> {
    let _ = def;
    let v = cond.value.as_ref().ok_or_else(|| {
        AppError::validation(format!("field `{}`: `{op}` needs {expected}", cond.field))
    })?;
    v.as_array().cloned().ok_or_else(|| {
        AppError::validation(format!("field `{}`: `{op}` needs {expected}", cond.field))
    })
}

fn coerce(def: &FieldDef, v: &serde_json::Value, cond: &FilterCondition) -> AppResult<SqlParam> {
    let field = cond.field.as_str();
    match def.kind {
        Kind::Real => {
            let f = v
                .as_f64()
                .filter(|x| x.is_finite())
                .ok_or_else(|| AppError::validation(format!("field `{field}` expects a number")))?;
            Ok(SqlParam::Real(f))
        }
        Kind::Int => {
            let n = v
                .as_i64()
                .ok_or_else(|| AppError::validation(format!("field `{field}` expects a whole number")))?;
            Ok(SqlParam::Int(n))
        }
        Kind::Text => {
            let s = v
                .as_str()
                .ok_or_else(|| AppError::validation(format!("field `{field}` expects text")))?
                .trim();
            Ok(SqlParam::Text(s.to_string()))
        }
        Kind::Bool => {
            let b = v
                .as_bool()
                .ok_or_else(|| AppError::validation(format!("field `{field}` expects true or false")))?;
            Ok(SqlParam::Bool(b))
        }
        Kind::DateTime => {
            // Stored as TEXT; lexicographic comparison holds because every
            // value is UTC RFC3339. Accept date-only or full timestamps.
            let s = v
                .as_str()
                .ok_or_else(|| AppError::validation(format!("field `{field}` expects a date or timestamp")))?
                .trim();
            if s.is_empty() {
                return Err(AppError::validation(format!(
                    "field `{field}` expects a date or timestamp"
                )));
            }
            Ok(SqlParam::Text(s.to_string()))
        }
    }
}

fn op_symbol(op: &CondOperator) -> &'static str {
    match op {
        CondOperator::Eq => "=",
        CondOperator::Neq => "!=",
        CondOperator::Gt => ">",
        CondOperator::Gte => ">=",
        CondOperator::Lt => "<",
        CondOperator::Lte => "<=",
        _ => unreachable!("build_clause handles multi-value ops separately"),
    }
}

fn op_name(op: &CondOperator) -> &'static str {
    match op {
        CondOperator::Eq => "=",
        CondOperator::Neq => "!=",
        CondOperator::Gt => ">",
        CondOperator::Gte => ">=",
        CondOperator::Lt => "<",
        CondOperator::Lte => "<=",
        CondOperator::Between => "between",
        CondOperator::In => "in",
        CondOperator::IsNull => "is-null",
        CondOperator::NotNull => "not-null",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cond(field: &str, op: &str, value: serde_json::Value) -> FilterCondition {
        let operator = match op {
            "=" => CondOperator::Eq,
            "!=" => CondOperator::Neq,
            ">" => CondOperator::Gt,
            ">=" => CondOperator::Gte,
            "<" => CondOperator::Lt,
            "<=" => CondOperator::Lte,
            "between" => CondOperator::Between,
            "in" => CondOperator::In,
            "is-null" => CondOperator::IsNull,
            "not-null" => CondOperator::NotNull,
            other => panic!("unexpected op {other}"),
        };
        FilterCondition {
            field: field.to_string(),
            operator,
            value: Some(value),
        }
    }

    fn build_conds(conds: Vec<FilterCondition>) -> (String, Vec<SqlParam>) {
        let f = Filter {
            operator: Operator::And,
            conditions: conds,
        };
        build_where(&f).unwrap()
    }

    #[test]
    fn empty_filter_yields_empty_where() {
        let (sql, params) = build_where(&Filter::empty()).unwrap();
        assert_eq!(sql, "");
        assert!(params.is_empty());
    }

    #[test]
    fn parse_rejects_garbage_with_friendly_error() {
        assert!(parse_filter("{nope").is_err());
        assert!(parse_filter(r#"{"operator":"OR","conditions":[]}"#).is_err());
        // operator name outside the serde set
        let bad = parse_filter(
            r#"{"operator":"AND","conditions":[{"field":"sharpness","operator":"like","value":5}]}"#,
        );
        assert!(bad.is_err());
        let ok = parse_filter(r#"{"operator":"AND","conditions":[]}"#).unwrap();
        assert!(ok.is_empty());
        assert!(parse_filter("").unwrap().is_empty());
    }

    #[test]
    fn unknown_field_fails_friendly() {
        let (sql, _) = build_conds(vec![cond("sharpness", ">=", json!(70))]);
        assert_eq!(sql, "WHERE a.sharpness >= ?");
        let f = Filter {
            operator: Operator::And,
            conditions: vec![cond("not_a_field", "=", json!(1))],
        };
        let err = build_where(&f).unwrap_err().to_string();
        assert!(err.contains("not_a_field"));
    }

    #[test]
    fn and_composition_and_param_order() {
        let (sql, params) = build_conds(vec![
            cond("sharpness", ">=", json!(70)),
            cond("orientation", "=", json!("portrait")),
            cond("iso", "<", json!(1600)),
        ]);
        assert_eq!(
            sql,
            "WHERE a.sharpness >= ? AND p.orientation = ? AND p.iso < ?"
        );
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn between_builds_range() {
        let (sql, params) = build_conds(vec![cond(
            "capture_datetime",
            "between",
            json!(["2026-01-01", "2026-12-31"]),
        )]);
        assert_eq!(
            sql,
            "WHERE p.capture_datetime >= ? AND p.capture_datetime <= ?"
        );
        assert_eq!(
            params,
            vec![
                SqlParam::Text("2026-01-01".into()),
                SqlParam::Text("2026-12-31".into()),
            ]
        );
    }

    #[test]
    fn in_builds_placeholders() {
        let (sql, params) = build_conds(vec![cond(
            "camera_model",
            "in",
            json!(["Gr-1", "Gr-33"]),
        )]);
        assert_eq!(sql, "WHERE p.camera_model IN (?, ?)");
        assert_eq!(
            params,
            vec![SqlParam::Text("Gr-1".into()), SqlParam::Text("Gr-33".into())]
        );
    }

    #[test]
    fn bool_flags_and_color_inverse() {
        let (sql, params) = build_conds(vec![
            cond("monochrome", "=", json!(true)),
            cond("color", "=", json!(true)),
        ]);
        assert_eq!(sql, "WHERE a.is_monochrome = ? AND a.is_monochrome = ?");
        assert_eq!(
            params,
            vec![SqlParam::Bool(true), SqlParam::Bool(false)]
        );
    }

    #[test]
    fn null_operators_need_no_value() {
        let (sql, params) = build_conds(vec![cond("lens", "is-null", json!(null))]);
        assert_eq!(sql, "WHERE p.lens IS NULL");
        assert!(params.is_empty());
    }

    #[test]
    fn bad_value_types_are_rejected() {
        let cases = vec![
            (vec![cond("sharpness", "=", json!("nope"))], "number"),
            (vec![cond("iso", "=", json!(1.5))], "whole number"),
            (vec![cond("orientation", "=", json!(42))], "text"),
            (vec![cond("monochrome", "=", json!("yes"))], "true or false"),
            (vec![cond("capture_datetime", "=", json!(123))], "date"),
            (vec![cond("contrast", "between", json!(1))], "[lo, hi]"),
            (vec![cond("contrast", "between", json!([1, 2, 3]))], "two values"),
            (vec![cond("camera_model", "in", json!([]))], "1..="),
        ];
        for (conds, want) in cases {
            let err = build_where(&Filter {
                operator: Operator::And,
                conditions: conds,
            })
            .unwrap_err()
            .to_string();
            assert!(err.contains(want), "expected '{want}' in: {err}");
        }
    }

    #[test]
    fn derived_ai_fields_use_count_expressions() {
        let (sql, _) = build_conds(vec![cond("faces_present", "=", json!(true))]);
        assert_eq!(
            sql,
            "WHERE (a.face_count IS NOT NULL AND a.face_count > 0) = ?"
        );
    }

    #[test]
    fn marking_fields_build_mark_sql() {
        // Rating (unrated = NULL) filters through the null operators.
        let (sql, _) = build_conds(vec![cond("rating", "not-null", json!(null))]);
        assert_eq!(sql, "WHERE p.rating IS NOT NULL");
        let (sql, params) = build_conds(vec![cond("rating", ">=", json!(3))]);
        assert_eq!(sql, "WHERE p.rating >= ?");
        assert_eq!(params.len(), 1);
        // Flagged maps to a derived bool expression.
        let (sql, _) = build_conds(vec![cond("flagged", "=", json!(true))]);
        assert_eq!(sql, "WHERE p.flag = 1 = ?");
        // Color label is plain text; `not-null` = has any color.
        let (sql, _) = build_conds(vec![cond("color_label", "not-null", json!(null))]);
        assert_eq!(sql, "WHERE p.color_label IS NOT NULL");
        let (sql, _) = build_conds(vec![cond("color_label", "=", json!("red"))]);
        assert_eq!(sql, "WHERE p.color_label = ?");
    }

    #[test]
    fn unsupported_operator_is_rejected() {
        let (sql_ok, _) = build_conds(vec![cond("iso", "not-null", json!(null))]);
        assert_eq!(sql_ok, "WHERE p.iso IS NOT NULL");
        // `in` on a bool flag is not meaningful
        let f = Filter {
            operator: Operator::And,
            conditions: vec![cond("dark", "in", json!([true]))],
        };
        let err = build_where(&f).unwrap_err().to_string();
        assert!(err.contains("does not support"));
    }

    #[test]
    fn condition_cap_enforced() {
        let mut conds = Vec::new();
        for _ in 0..(MAX_CONDITIONS + 1) {
            conds.push(cond("lens", "not-null", json!(null)));
        }
        let f = serde_json::json!({ "operator": "AND", "conditions": conds });
        let f: Filter = serde_json::from_value(f).unwrap();
        // parse_filter enforces the cap on the JSON path; direct structs
        // bypass it (they are only built by parse_filter in practice).
        let n = f.conditions.len();
        assert_eq!(n, MAX_CONDITIONS + 1);
        let big = serde_json::json!({ "operator": "AND", "conditions": conds }).to_string();
        assert!(parse_filter(&big).is_err());
    }
}
