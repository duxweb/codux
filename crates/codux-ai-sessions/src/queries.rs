use super::{
    helpers::{deterministic_uuid, history_group_key, table_has_column},
    types::{
        AIProjectUsageSummary, AISessionSummary, AIUsageAmount, SessionDetailLink, SessionLink,
    },
};
use codux_ai_history::normalized::{AIHeatmapDay, AITimeBucket, AIUsageBreakdownItem};
use rusqlite::{Connection, params};
use std::collections::{BTreeMap, HashMap};

pub(super) fn load_sessions(
    conn: &Connection,
    project_path: &str,
) -> Result<Vec<AISessionSummary>, String> {
    let input_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "input_tokens") {
            "COALESCE(SUM(b.input_tokens), 0)"
        } else {
            "0"
        };
    let output_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "output_tokens") {
            "COALESCE(SUM(b.output_tokens), 0)"
        } else {
            "0"
        };
    let mut statement = conn
        .prepare(&format!(
            r#"
            SELECT
                l.session_key,
                l.external_session_id,
                l.session_title,
                l.source,
                l.last_model,
                l.last_seen_at,
                {input_tokens_expr} AS input_tokens,
                {output_tokens_expr} AS output_tokens,
                COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
                COALESCE(SUM(b.cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(b.request_count), 0) AS request_count
            FROM ai_history_file_session_link l
            LEFT JOIN ai_history_file_usage_bucket b
                ON b.project_path = l.project_path
                AND b.source = l.source
                AND b.file_path = l.file_path
                AND b.session_key = l.session_key
            WHERE l.project_path = ?1
            GROUP BY l.session_key, l.external_session_id, l.session_title, l.source, l.last_model, l.last_seen_at
            ORDER BY l.last_seen_at DESC
            LIMIT 12
            "#
        ))
        .map_err(|error| error.to_string())?;

    let usage_amounts = load_session_usage_amounts(conn, Some(project_path))?;
    let rows = statement
        .query_map([project_path], |row| {
            let session_key = row.get::<_, String>(0)?;
            let external_session_id = row.get::<_, Option<String>>(1)?;
            let source = row.get::<_, String>(3)?;
            let amounts = usage_amounts
                .get(&(
                    source.clone(),
                    session_key.clone(),
                    external_session_id.clone(),
                ))
                .cloned()
                .unwrap_or_default();
            Ok(AISessionSummary {
                id: deterministic_uuid(&history_group_key(
                    &source,
                    &session_key,
                    external_session_id.as_deref(),
                )),
                session_key,
                external_session_id,
                title: row.get(2)?,
                source,
                project_name: Some(project_path.to_string()),
                project_path: Some(project_path.to_string()),
                last_model: row.get(4)?,
                last_seen_at: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                total_tokens: row.get(8)?,
                cached_input_tokens: row.get(9)?,
                request_count: row.get(10)?,
                active_duration_seconds: 0,
                usage_amounts: amounts,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn load_global_recent_sessions(
    conn: &Connection,
) -> Result<Vec<AISessionSummary>, String> {
    let project_name_expr =
        if table_has_column(conn, "ai_history_file_session_link", "project_name") {
            "COALESCE(NULLIF(MAX(l.project_name), ''), MAX(l.project_path))"
        } else {
            "MAX(l.project_path)"
        };
    let input_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "input_tokens") {
            "COALESCE(SUM(b.input_tokens), 0)"
        } else {
            "0"
        };
    let output_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "output_tokens") {
            "COALESCE(SUM(b.output_tokens), 0)"
        } else {
            "0"
        };
    let sql = format!(
        r#"
        SELECT
            l.session_key,
            l.external_session_id,
            l.session_title,
            l.source,
            {project_name_expr} AS project_name,
            MAX(l.project_path) AS project_path,
            l.last_model,
            MAX(l.last_seen_at) AS last_seen_at,
            {input_tokens_expr} AS input_tokens,
            {output_tokens_expr} AS output_tokens,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cached_input_tokens), 0) AS cached_input_tokens,
            COALESCE(SUM(b.request_count), 0) AS request_count
        FROM ai_history_file_session_link l
        LEFT JOIN ai_history_file_usage_bucket b
            ON b.project_path = l.project_path
            AND b.source = l.source
            AND b.file_path = l.file_path
            AND b.session_key = l.session_key
        GROUP BY l.source, l.session_key, l.external_session_id, l.session_title, l.last_model
        ORDER BY last_seen_at DESC
        LIMIT 80
        "#
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;

    let usage_amounts = load_session_usage_amounts(conn, None)?;
    let rows = statement
        .query_map([], |row| {
            let session_key = row.get::<_, String>(0)?;
            let external_session_id = row.get::<_, Option<String>>(1)?;
            let source = row.get::<_, String>(3)?;
            let amounts = usage_amounts
                .get(&(
                    source.clone(),
                    session_key.clone(),
                    external_session_id.clone(),
                ))
                .cloned()
                .unwrap_or_default();
            Ok(AISessionSummary {
                id: deterministic_uuid(&history_group_key(
                    &source,
                    &session_key,
                    external_session_id.as_deref(),
                )),
                session_key,
                external_session_id,
                title: row.get(2)?,
                source,
                project_name: row.get(4)?,
                project_path: row.get(5)?,
                last_model: row.get(6)?,
                last_seen_at: row.get(7)?,
                input_tokens: row.get(8)?,
                output_tokens: row.get(9)?,
                total_tokens: row.get(10)?,
                cached_input_tokens: row.get(11)?,
                request_count: row.get(12)?,
                active_duration_seconds: 0,
                usage_amounts: amounts,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

type SessionUsageAmountMap = HashMap<(String, String, Option<String>), Vec<AIUsageAmount>>;

fn load_session_usage_amounts(
    conn: &Connection,
    project_path: Option<&str>,
) -> Result<SessionUsageAmountMap, String> {
    if !table_exists(conn, "ai_history_file_usage_amount") {
        return Ok(SessionUsageAmountMap::new());
    }
    let where_clause = if project_path.is_some() {
        "WHERE a.project_path = ?1"
    } else {
        ""
    };
    let sql = format!(
        r#"
        SELECT
            a.source,
            a.session_key,
            l.external_session_id,
            a.unit,
            COALESCE(SUM(a.value), 0.0) AS value
        FROM ai_history_file_usage_amount a
        LEFT JOIN ai_history_file_session_link l
            ON l.project_path = a.project_path
            AND l.source = a.source
            AND l.file_path = a.file_path
            AND l.session_key = a.session_key
        {where_clause}
        GROUP BY a.source, a.session_key, l.external_session_id, a.unit
        "#
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = if let Some(project_path) = project_path {
        statement
            .query_map([project_path], session_usage_amount_row)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    } else {
        statement
            .query_map([], session_usage_amount_row)
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?
    };
    let mut map = SessionUsageAmountMap::new();
    for (source, session_key, external_session_id, amount) in rows {
        if amount.value <= 0.0 || amount.unit.trim().is_empty() {
            continue;
        }
        let amounts = map
            .entry((source, session_key, external_session_id))
            .or_default();
        if let Some(existing) = amounts.iter_mut().find(|item| item.unit == amount.unit) {
            existing.value += amount.value;
        } else {
            amounts.push(amount);
        }
    }
    Ok(map)
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_| Ok(()),
    )
    .is_ok()
}

fn session_usage_amount_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(String, String, Option<String>, AIUsageAmount)> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        AIUsageAmount {
            unit: row.get(3)?,
            value: row.get(4)?,
        },
    ))
}

pub(super) fn load_global_project_totals(
    conn: &Connection,
    today_start: f64,
) -> Result<Vec<AIProjectUsageSummary>, String> {
    let project_name_expr =
        if table_has_column(conn, "ai_history_project_index_state", "project_name") {
            "COALESCE(NULLIF(MAX(p.project_name), ''), l.project_path)"
        } else {
            "l.project_path"
        };
    let project_id_expr = if table_has_column(conn, "ai_history_project_index_state", "project_id")
    {
        "COALESCE(NULLIF(MAX(p.project_id), ''), l.project_path)"
    } else {
        "l.project_path"
    };
    let input_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "input_tokens") {
            "COALESCE(SUM(b.input_tokens), 0)"
        } else {
            "0"
        };
    let output_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "output_tokens") {
            "COALESCE(SUM(b.output_tokens), 0)"
        } else {
            "0"
        };
    let sql = format!(
        r#"
        SELECT
            {project_id_expr} AS project_id,
            l.project_path,
            {project_name_expr} AS project_name,
            COUNT(DISTINCT l.source || ':' || l.session_key) AS session_count,
            {input_tokens_expr} AS input_tokens,
            {output_tokens_expr} AS output_tokens,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cached_input_tokens), 0) AS cached_input_tokens,
            COALESCE(SUM(b.request_count), 0) AS request_count,
            COALESCE(SUM(DISTINCT l.active_duration_seconds), 0) AS active_duration_seconds,
            COALESCE(SUM(CASE WHEN b.bucket_start >= ?1 THEN b.total_tokens ELSE 0 END), 0) AS today_total_tokens,
            COALESCE(SUM(CASE WHEN b.bucket_start >= ?2 THEN b.cached_input_tokens ELSE 0 END), 0) AS today_cached_input_tokens
        FROM ai_history_file_session_link l
        LEFT JOIN ai_history_project_index_state p
            ON p.project_path = l.project_path
        LEFT JOIN ai_history_file_usage_bucket b
            ON b.project_path = l.project_path
            AND b.source = l.source
            AND b.file_path = l.file_path
            AND b.session_key = l.session_key
        GROUP BY l.project_path
        ORDER BY total_tokens DESC, l.project_path ASC
        "#
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([today_start, today_start], |row| {
            Ok(AIProjectUsageSummary {
                project_id: row.get(0)?,
                project_path: row.get(1)?,
                project_name: row.get(2)?,
                session_count: row.get::<_, i64>(3)?.max(0) as usize,
                input_tokens: row.get(4)?,
                output_tokens: row.get(5)?,
                total_tokens: row.get(6)?,
                cached_input_tokens: row.get(7)?,
                request_count: row.get(8)?,
                active_duration_seconds: row.get(9)?,
                today_total_tokens: row.get(10)?,
                today_cached_input_tokens: row.get(11)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn load_session_detail_links(
    conn: &Connection,
    project_path: &str,
) -> Result<Vec<SessionDetailLink>, String> {
    let has_first_seen = table_has_column(conn, "ai_history_file_session_link", "first_seen_at");
    let has_active_duration = table_has_column(
        conn,
        "ai_history_file_session_link",
        "active_duration_seconds",
    );
    let first_seen_expr = if has_first_seen {
        "first_seen_at"
    } else {
        "last_seen_at"
    };
    let active_duration_expr = if has_active_duration {
        "active_duration_seconds"
    } else {
        "0"
    };
    let sql = format!(
        r#"
        SELECT
            source,
            file_path,
            session_key,
            external_session_id,
            session_title,
            {first_seen_expr},
            last_seen_at,
            last_model,
            {active_duration_expr}
        FROM ai_history_file_session_link
        WHERE project_path = ?1
        ORDER BY last_seen_at DESC
        "#
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([project_path], |row| {
            Ok(SessionDetailLink {
                source: row.get(0)?,
                file_path: row.get(1)?,
                session_key: row.get(2)?,
                external_session_id: row.get(3)?,
                title: row.get(4)?,
                first_seen_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                last_model: row.get(7)?,
                active_duration_seconds: row.get(8)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn load_global_today_cached_tokens(
    conn: &Connection,
    today_start: f64,
) -> Result<i64, String> {
    conn.query_row(
        r#"
        SELECT COALESCE(SUM(cached_input_tokens), 0)
        FROM ai_history_file_usage_bucket
        WHERE bucket_start >= ?1
        "#,
        [today_start],
        |row| row.get(0),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn load_file_usage(
    conn: &Connection,
    project_path: &str,
    source: &str,
    file_path: &str,
    session_key: &str,
) -> Result<(i64, i64, i64), String> {
    conn.query_row(
        r#"
        SELECT
            COALESCE(SUM(total_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0),
            COALESCE(SUM(request_count), 0)
        FROM ai_history_file_usage_bucket
        WHERE project_path = ?1 AND source = ?2 AND file_path = ?3 AND session_key = ?4
        "#,
        params![project_path, source, file_path, session_key],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn load_session_links(
    conn: &Connection,
    project_path: &str,
) -> Result<Vec<SessionLink>, String> {
    let mut statement = conn
        .prepare(
            r#"
            SELECT source, session_key, external_session_id
            FROM ai_history_file_session_link
            WHERE project_path = ?1
            ORDER BY last_seen_at DESC
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([project_path], |row| {
            Ok(SessionLink {
                source: row.get(0)?,
                session_key: row.get(1)?,
                external_session_id: row.get(2)?,
            })
        })
        .map_err(|error| error.to_string())?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub(super) fn load_today_tokens(
    conn: &Connection,
    project_path: &str,
    today_start: f64,
) -> Result<(i64, i64), String> {
    conn.query_row(
        r#"
        SELECT
            COALESCE(SUM(total_tokens), 0),
            COALESCE(SUM(cached_input_tokens), 0)
        FROM ai_history_file_usage_bucket
        WHERE project_path = ?1 AND bucket_start >= ?2
        "#,
        (project_path, today_start),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .map_err(|error| error.to_string())
}

pub(super) fn load_project_aggregates(
    conn: &Connection,
    project_path: &str,
    today_start: f64,
) -> Result<ProjectHistoryAggregates, String> {
    Ok(ProjectHistoryAggregates {
        heatmap: load_heatmap(conn, project_path)?,
        today_time_buckets: load_today_time_buckets(conn, project_path, today_start)?,
        tool_breakdown: load_breakdown(conn, project_path, "source")?,
        model_breakdown: load_breakdown(conn, project_path, "model")?,
    })
}

#[derive(Default)]
pub(super) struct ProjectHistoryAggregates {
    pub(super) heatmap: Vec<AIHeatmapDay>,
    pub(super) today_time_buckets: Vec<AITimeBucket>,
    pub(super) tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub(super) model_breakdown: Vec<AIUsageBreakdownItem>,
}

fn load_heatmap(conn: &Connection, project_path: &str) -> Result<Vec<AIHeatmapDay>, String> {
    let input_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "input_tokens") {
            "COALESCE(SUM(input_tokens), 0)"
        } else {
            "0"
        };
    let output_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "output_tokens") {
            "COALESCE(SUM(output_tokens), 0)"
        } else {
            "0"
        };
    let mut statement = conn
        .prepare(&format!(
            r#"
            SELECT
                bucket_start,
                {input_tokens_expr} AS input_tokens,
                {output_tokens_expr} AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket
            WHERE project_path = ?1
            GROUP BY bucket_start
            ORDER BY bucket_start ASC
            "#,
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([project_path], |row| {
            Ok((
                row.get::<_, f64>(0)?,
                row.get::<_, i64>(1)?.max(0),
                row.get::<_, i64>(2)?.max(0),
                row.get::<_, i64>(3)?.max(0),
                row.get::<_, i64>(4)?.max(0),
                row.get::<_, i64>(5)?.max(0),
            ))
        })
        .map_err(|error| error.to_string())?;
    let rows = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(heatmap_days_from_buckets(rows))
}

fn load_today_time_buckets(
    conn: &Connection,
    project_path: &str,
    today_start: f64,
) -> Result<Vec<AITimeBucket>, String> {
    let mut rows_by_start = BTreeMap::<i64, AITimeBucket>::new();
    let mut statement = conn
        .prepare(
            r#"
            SELECT
                bucket_start,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket
            WHERE project_path = ?1 AND bucket_start >= ?2
            GROUP BY bucket_start
            ORDER BY bucket_start ASC
            "#,
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map((project_path, today_start), |row| {
            let start = row.get::<_, f64>(0)?;
            Ok(AITimeBucket {
                start,
                end: start + 30.0 * 60.0,
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: row.get(1)?,
                cached_input_tokens: row.get(2)?,
                request_count: row.get(3)?,
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        rows_by_start.insert(row.start as i64, row);
    }

    Ok((0..48)
        .map(|index| {
            let start = today_start + f64::from(index) * 30.0 * 60.0;
            rows_by_start
                .remove(&(start as i64))
                .unwrap_or(AITimeBucket {
                    start,
                    end: start + 30.0 * 60.0,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                })
        })
        .collect())
}

pub(super) fn load_global_today_time_buckets(
    conn: &Connection,
    today_start: f64,
) -> Result<Vec<AITimeBucket>, String> {
    load_global_time_buckets(conn, today_start, today_start)
}

pub(super) fn load_global_recent_time_buckets(
    conn: &Connection,
    range_start: f64,
) -> Result<Vec<AITimeBucket>, String> {
    load_global_time_buckets(conn, range_start, range_start)
}

fn load_global_time_buckets(
    conn: &Connection,
    query_start: f64,
    display_start: f64,
) -> Result<Vec<AITimeBucket>, String> {
    let input_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "input_tokens") {
            "COALESCE(SUM(input_tokens), 0)"
        } else {
            "0"
        };
    let output_tokens_expr =
        if table_has_column(conn, "ai_history_file_usage_bucket", "output_tokens") {
            "COALESCE(SUM(output_tokens), 0)"
        } else {
            "0"
        };
    let mut rows_by_start = BTreeMap::<i64, AITimeBucket>::new();
    let mut statement = conn
        .prepare(&format!(
            r#"
            SELECT
                bucket_start,
                {input_tokens_expr} AS input_tokens,
                {output_tokens_expr} AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cached_input_tokens), 0) AS cached_input_tokens,
                COALESCE(SUM(request_count), 0) AS request_count
            FROM ai_history_file_usage_bucket
            WHERE bucket_start >= ?1
            GROUP BY bucket_start
            ORDER BY bucket_start ASC
            "#
        ))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([query_start], |row| {
            let start = row.get::<_, f64>(0)?;
            Ok(AITimeBucket {
                start,
                end: start + 30.0 * 60.0,
                input_tokens: row.get::<_, i64>(1)?.max(0),
                output_tokens: row.get::<_, i64>(2)?.max(0),
                total_tokens: row.get::<_, i64>(3)?.max(0),
                cached_input_tokens: row.get::<_, i64>(4)?.max(0),
                request_count: row.get::<_, i64>(5)?.max(0),
            })
        })
        .map_err(|error| error.to_string())?;
    for row in rows {
        let row = row.map_err(|error| error.to_string())?;
        rows_by_start.insert(row.start as i64, row);
    }

    Ok((0..48)
        .map(|index| {
            let start = display_start + f64::from(index) * 30.0 * 60.0;
            rows_by_start
                .remove(&(start as i64))
                .unwrap_or(AITimeBucket {
                    start,
                    end: start + 30.0 * 60.0,
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                })
        })
        .collect())
}

fn load_breakdown(
    conn: &Connection,
    project_path: &str,
    group_key: &str,
) -> Result<Vec<AIUsageBreakdownItem>, String> {
    let group_expr = match group_key {
        "model" => "COALESCE(NULLIF(l.last_model, ''), 'unknown')",
        _ => "b.source",
    };
    let sql = format!(
        r#"
        SELECT
            {group_expr} AS item_key,
            COALESCE(SUM(b.total_tokens), 0) AS total_tokens,
            COALESCE(SUM(b.cached_input_tokens), 0) AS cached_input_tokens,
            COALESCE(SUM(b.request_count), 0) AS request_count
        FROM ai_history_file_usage_bucket b
        LEFT JOIN ai_history_file_session_link l
            ON l.project_path = b.project_path
            AND l.source = b.source
            AND l.file_path = b.file_path
            AND l.session_key = b.session_key
        WHERE b.project_path = ?1
        GROUP BY item_key
        ORDER BY total_tokens DESC, item_key ASC
        "#
    );
    let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([project_path], |row| {
            Ok(AIUsageBreakdownItem {
                key: row.get(0)?,
                total_tokens: row.get(1)?,
                cached_input_tokens: row.get(2)?,
                request_count: row.get(3)?,
                usage_amounts: Vec::new(),
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn heatmap_days_from_buckets(rows: Vec<(f64, i64, i64, i64, i64, i64)>) -> Vec<AIHeatmapDay> {
    let mut days = BTreeMap::<i64, AIHeatmapDay>::new();
    for (
        bucket_start,
        input_tokens,
        output_tokens,
        total_tokens,
        cached_input_tokens,
        request_count,
    ) in rows
    {
        let day = codux_ai_history::normalized::local_day_start_seconds(bucket_start);
        let item = days.entry(day as i64).or_insert(AIHeatmapDay {
            day,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        });
        item.input_tokens += input_tokens;
        item.output_tokens += output_tokens;
        item.total_tokens += total_tokens;
        item.cached_input_tokens += cached_input_tokens;
        item.request_count += request_count;
    }
    days.into_values().collect()
}
