impl AIUsageStore {
    fn external_file_checkpoint(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<Option<AIExternalFileCheckpoint>> {
        conn.query_row(
            r#"
            SELECT file_modified_at, file_size, last_offset, last_indexed_at, payload_json
            FROM ai_history_file_checkpoint
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            LIMIT 1;
            "#,
            params![source, file_path, project_path],
            |row| {
                Ok(AIExternalFileCheckpoint {
                    source: source.to_string(),
                    file_path: file_path.to_string(),
                    project_path: project_path.to_string(),
                    file_modified_at: row.get(0)?,
                    file_size: row.get(1)?,
                    last_offset: row.get(2)?,
                    last_indexed_at: row.get(3)?,
                    payload_json: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    fn load_usage_buckets(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<Vec<AIUsageBucket>> {
        let session_links = self.load_session_links(conn, source, file_path, project_path)?;
        let mut statement = conn.prepare(
            r#"
            SELECT session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
                   total_tokens, cached_input_tokens, request_count, active_duration_seconds
            FROM ai_history_file_usage_bucket
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            ORDER BY bucket_start ASC, session_key ASC, model ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![source, file_path, project_path], |row| {
                Ok(StoredUsageBucketRow {
                    source: source.to_string(),
                    session_key: row.get(0)?,
                    model: normalized_optional_string(row.get::<_, String>(1)?.as_str()),
                    bucket_start: row.get(2)?,
                    bucket_end: row.get(3)?,
                    input_tokens: row.get(4)?,
                    output_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                    cached_input_tokens: row.get(7)?,
                    request_count: row.get(8)?,
                    active_duration_seconds: row.get(9)?,
                    usage_amounts: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let usage_amounts =
            self.load_usage_amounts_by_bucket(conn, source, file_path, project_path)?;
        let usage_buckets = rows
            .into_iter()
            .filter_map(|row| {
                let session = session_links.get(&row.session_key)?;
                let model_key = row.model.clone().unwrap_or_default();
                let amount_key = (row.session_key.clone(), model_key, row.bucket_start as i64);
                Some(AIUsageBucket {
                    source: row.source,
                    session_key: row.session_key,
                    external_session_id: session.external_session_id.clone(),
                    session_title: session.session_title.clone(),
                    model: row.model,
                    project_id: session.project_id.clone(),
                    project_name: session.project_name.clone(),
                    bucket_start: row.bucket_start,
                    bucket_end: row.bucket_end,
                    input_tokens: row.input_tokens,
                    output_tokens: row.output_tokens,
                    total_tokens: row.total_tokens,
                    cached_input_tokens: row.cached_input_tokens,
                    usage_amounts: usage_amounts.get(&amount_key).cloned().unwrap_or_default(),
                    request_count: row.request_count,
                    active_duration_seconds: row.active_duration_seconds,
                    first_seen_at: session.first_seen_at,
                    last_seen_at: session.last_seen_at,
                })
            })
            .collect();
        Ok(usage_buckets)
    }

    fn load_usage_amounts_by_bucket(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<HashMap<(String, String, i64), Vec<AIUsageAmount>>> {
        let mut statement = conn.prepare(
            r#"
            SELECT session_key, model, bucket_start, unit, value
            FROM ai_history_file_usage_amount
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            ORDER BY bucket_start ASC, session_key ASC, model ASC, unit ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![source, file_path, project_path], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, f64>(2)? as i64,
                    AIUsageAmount {
                        unit: row.get(3)?,
                        value: row.get(4)?,
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut map = HashMap::<(String, String, i64), Vec<AIUsageAmount>>::new();
        for (session_key, model, bucket_start, amount) in rows {
            merge_usage_amount(
                map.entry((session_key, model, bucket_start))
                    .or_default(),
                amount,
            );
        }
        Ok(map)
    }

    fn load_session_links(
        &self,
        conn: &Connection,
        source: &str,
        file_path: &str,
        project_path: &str,
    ) -> Result<HashMap<String, NormalizedSessionLinkRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT session_key, external_session_id, project_id, project_name, session_title,
                   first_seen_at, last_seen_at, last_model, active_duration_seconds
            FROM ai_history_file_session_link
            WHERE source = ?1 AND file_path = ?2 AND project_path = ?3
            ORDER BY last_seen_at DESC;
            "#,
        )?;
        let rows = statement.query_map(params![source, file_path, project_path], |row| {
            Ok(NormalizedSessionLinkRow {
                source: source.to_string(),
                session_key: row.get(0)?,
                external_session_id: row.get(1)?,
                project_id: row.get(2)?,
                project_name: row.get(3)?,
                session_title: row.get(4)?,
                first_seen_at: row.get(5)?,
                last_seen_at: row.get(6)?,
                last_model: row.get(7)?,
                active_duration_seconds: row.get(8)?,
            })
        })?;
        let mut map = HashMap::new();
        for row in rows {
            let row = row?;
            map.insert(row.session_key.clone(), row);
        }
        Ok(map)
    }

    fn project_session_links(
        &self,
        conn: &Connection,
        project_path: &str,
    ) -> Result<Vec<NormalizedSessionLinkRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT source, file_path, project_path, session_key, external_session_id,
                   project_id, project_name, session_title, first_seen_at, last_seen_at,
                   last_model, active_duration_seconds
            FROM ai_history_file_session_link
            WHERE project_path = ?1
            ORDER BY last_seen_at DESC;
            "#,
        )?;
        statement
            .query_map(params![project_path], |row| {
                Ok(NormalizedSessionLinkRow {
                    source: row.get(0)?,
                    session_key: row.get(3)?,
                    external_session_id: row.get(4)?,
                    project_id: row.get(5)?,
                    project_name: row.get(6)?,
                    session_title: row.get(7)?,
                    first_seen_at: row.get(8)?,
                    last_seen_at: row.get(9)?,
                    last_model: row.get(10)?,
                    active_duration_seconds: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    fn project_usage_buckets(
        &self,
        conn: &Connection,
        project_path: &str,
    ) -> Result<Vec<StoredUsageBucketRow>> {
        let mut statement = conn.prepare(
            r#"
            SELECT source, session_key, model, bucket_start, bucket_end, input_tokens, output_tokens,
                   total_tokens, cached_input_tokens, request_count, active_duration_seconds
            FROM ai_history_file_usage_bucket
            WHERE project_path = ?1
            ORDER BY bucket_start ASC, source ASC, session_key ASC, model ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![project_path], |row| {
                Ok(StoredUsageBucketRow {
                    source: row.get(0)?,
                    session_key: row.get(1)?,
                    model: normalized_optional_string(row.get::<_, String>(2)?.as_str()),
                    bucket_start: row.get(3)?,
                    bucket_end: row.get(4)?,
                    input_tokens: row.get(5)?,
                    output_tokens: row.get(6)?,
                    total_tokens: row.get(7)?,
                    cached_input_tokens: row.get(8)?,
                    request_count: row.get(9)?,
                    active_duration_seconds: row.get(10)?,
                    usage_amounts: Vec::new(),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut rows = rows;
        let amounts = self.project_usage_amounts_by_bucket(conn, project_path)?;
        for row in &mut rows {
            let key = (
                row.source.clone(),
                row.session_key.clone(),
                row.model.clone().unwrap_or_default(),
                row.bucket_start as i64,
            );
            row.usage_amounts = amounts.get(&key).cloned().unwrap_or_default();
        }
        Ok(rows)
    }

    fn project_usage_amounts_by_bucket(
        &self,
        conn: &Connection,
        project_path: &str,
    ) -> Result<HashMap<UsageBucketKey, Vec<AIUsageAmount>>> {
        let mut statement = conn.prepare(
            r#"
            SELECT source, session_key, model, bucket_start, unit, value
            FROM ai_history_file_usage_amount
            WHERE project_path = ?1
            ORDER BY bucket_start ASC, source ASC, session_key ASC, model ASC, unit ASC;
            "#,
        )?;
        let rows = statement
            .query_map(params![project_path], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)? as i64,
                    AIUsageAmount {
                        unit: row.get(4)?,
                        value: row.get(5)?,
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut map = HashMap::<UsageBucketKey, Vec<AIUsageAmount>>::new();
        for (source, session_key, model, bucket_start, amount) in rows {
            merge_usage_amount(
                map.entry((source, session_key, model, bucket_start))
                    .or_default(),
                amount,
            );
        }
        Ok(map)
    }
}
