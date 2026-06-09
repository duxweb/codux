type HistorySourcePathsFn = fn(&AIHistoryProjectRequest, &Path) -> Vec<PathBuf>;
type HistorySourceFileParserFn = fn(&AIHistoryProjectRequest, &Path) -> ParsedHistory;
type HistorySourceJsonlParserFn = fn(
    &AIHistoryProjectRequest,
    &Path,
    i64,
    Option<&AIExternalFileCheckpointPayload>,
) -> JSONLParseSnapshot;

struct HistorySourceDriver {
    source: &'static str,
    kind: HistorySourceDriverKind,
}

enum HistorySourceDriverKind {
    File {
        paths: HistorySourcePathsFn,
        parse_file: HistorySourceFileParserFn,
    },
    Jsonl {
        paths: HistorySourcePathsFn,
        parse_snapshot: HistorySourceJsonlParserFn,
    },
}

impl HistorySourceDriver {
    fn paths(&self, project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
        match self.kind {
            HistorySourceDriverKind::File { paths, .. }
            | HistorySourceDriverKind::Jsonl { paths, .. } => paths(project, home),
        }
    }

    fn parse_all(&self, project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
        let mut result = ParsedHistory::default();
        for file_path in self.paths(project, home) {
            result.merge(match self.kind {
                HistorySourceDriverKind::File { parse_file, .. } => parse_file(project, &file_path),
                HistorySourceDriverKind::Jsonl { parse_snapshot, .. } => {
                    parse_snapshot(project, &file_path, 0, None).result
                }
            });
        }
        result
    }

    fn load_or_index(
        &self,
        store: &AIUsageStore,
        conn: &Connection,
        project: &AIHistoryProjectRequest,
        home: &Path,
    ) -> Result<()> {
        for file_path in self.paths(project, home) {
            match self.kind {
                HistorySourceDriverKind::File { parse_file, .. } => {
                    let _ = store.load_or_index_file(conn, self.source, &file_path, project, || {
                        parse_file(project, &file_path)
                    })?;
                }
                HistorySourceDriverKind::Jsonl { parse_snapshot, .. } => {
                    let _ = store.load_or_index_jsonl_file(
                        conn,
                        self.source,
                        &file_path,
                        project,
                        |checkpoint| {
                            let seed = checkpoint.and_then(|checkpoint| {
                                decode_checkpoint_payload(checkpoint.payload_json.as_deref())
                            });
                            parse_snapshot(
                                project,
                                &file_path,
                                checkpoint.map(|item| item.last_offset).unwrap_or(0),
                                seed.as_ref(),
                            )
                        },
                        || parse_snapshot(project, &file_path, 0, None),
                    )?;
                }
            }
        }
        Ok(())
    }
}

const fn file_history_source_driver(
    source: &'static str,
    paths: HistorySourcePathsFn,
    parse_file: HistorySourceFileParserFn,
) -> HistorySourceDriver {
    HistorySourceDriver {
        source,
        kind: HistorySourceDriverKind::File { paths, parse_file },
    }
}

const fn jsonl_history_source_driver(
    source: &'static str,
    paths: HistorySourcePathsFn,
    parse_snapshot: HistorySourceJsonlParserFn,
) -> HistorySourceDriver {
    HistorySourceDriver {
        source,
        kind: HistorySourceDriverKind::Jsonl {
            paths,
            parse_snapshot,
        },
    }
}
