// Stage 1 of the damage-driven terminal renderer.
//
// Today the renderer re-walks the published content and re-hashes every visible
// row on every frame to key its prepared-row cache — even on frames where the
// content did not change (cursor blink, a sibling repaint, scrolling over
// unchanged scrollback). This module derives a *stable per-row version* by
// diffing successive published snapshots so the renderer can look a row up by
// version instead of re-hashing it each frame, and reuse prepared rows for the
// common case where a row's content is unchanged.
//
// The key invariant is that a version is tied to an absolute terminal line and
// its content hash: while output streams, existing lines scroll up but keep
// their content (and absolute line number), so their version is stable and only
// the newly written line(s) get a fresh version. The same holds while the user
// scrolls the viewport over unchanged scrollback.

/// Per-row content signatures (absolute `line` + content `hash`) for a content
/// snapshot, in first-seen display order. Lines with no cells are omitted,
/// matching the sparse cell model. Computed once per published content, not per
/// frame.
#[allow(dead_code)] // wired into the renderer cache in the next Stage 1 increment
fn terminal_content_row_signatures(content: &TerminalContent) -> Vec<(i32, u64)> {
    let mut order: Vec<i32> = Vec::new();
    let mut hashers: HashMap<i32, DefaultHasher> = HashMap::new();
    for indexed in &content.cells {
        let line = indexed.line();
        let hasher = hashers.entry(line).or_default();
        if order.last().is_none_or(|last| *last != line) && !order.contains(&line) {
            order.push(line);
        }
        terminal_cell_hash(&indexed.cell, hasher);
    }
    order
        .into_iter()
        .map(|line| {
            let hash = hashers
                .get(&line)
                .map(|hasher| hasher.clone().finish())
                .unwrap_or(0);
            (line, hash)
        })
        .collect()
}

/// Tracks a stable per-row version across published terminal snapshots.
///
/// `assign` takes the current rows (absolute `line` + content `hash`, in display
/// order) and returns one version per row. A line whose `(line, hash)` matches
/// the previous snapshot keeps its version; a line whose content changed, or a
/// newly-exposed line, gets a fresh, monotonically increasing version.
#[derive(Default)]
struct RowVersionTracker {
    // absolute line -> (content hash, assigned version) from the last snapshot
    previous: HashMap<i32, (u64, u64)>,
    next_version: u64,
}

impl RowVersionTracker {
    #[allow(dead_code)] // wired into the publish path in the next Stage 1 increment
    fn assign(&mut self, rows: &[(i32, u64)]) -> Vec<u64> {
        let mut next: HashMap<i32, (u64, u64)> = HashMap::with_capacity(rows.len());
        let mut versions = Vec::with_capacity(rows.len());
        for &(line, hash) in rows {
            // Reuse the version only when the *previous* snapshot had this line
            // with the same content; a duplicate line within one snapshot (which
            // should not happen for a well-formed grid) falls through to a fresh
            // version rather than aliasing.
            let version = match self.previous.get(&line) {
                Some(&(prev_hash, prev_version)) if prev_hash == hash => prev_version,
                _ => {
                    self.next_version = self.next_version.wrapping_add(1);
                    self.next_version
                }
            };
            next.insert(line, (hash, version));
            versions.push(version);
        }
        self.previous = next;
        versions
    }
}

#[cfg(test)]
mod grid_version_tests {
    use super::*;

    fn test_indexed_cell(line: i32, col: usize, text: &str) -> TerminalIndexedCell {
        TerminalIndexedCell {
            point: TerminalPoint { line, column: col },
            cell: TerminalScreenCellSnapshot {
                row: line,
                col,
                text: text.to_string(),
                width: 1,
                fg: TerminalScreenColor::Default,
                bg: TerminalScreenColor::Default,
                bold: false,
                dim: false,
                italic: false,
                underline: TerminalScreenUnderline::None,
                underline_color: None,
                link: None,
                inverse: false,
                hidden: false,
                strikeout: false,
            },
        }
    }

    fn test_text_run(start_col: usize, width_cols: usize, text: &str, bold: bool) -> TerminalTextRun {
        let fonts = TerminalFonts::new("test-mono");
        let style = TextRun {
            len: text.len(),
            font: fonts.get(bold, false),
            color: Hsla {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                a: 1.0,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        TerminalTextRun::from_text(0, start_col, text.to_string(), width_cols, style)
    }

    fn test_gap_font() -> Font {
        TerminalFonts::new("test-mono").get(false, false)
    }

    fn test_gap_color() -> Hsla {
        Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.5,
            a: 1.0,
        }
    }

    #[test]
    fn combine_merges_contiguous_spans_into_one_line() {
        // Two adjacent style spans ("ab" then "CD") on the same row become one
        // shaped line with two runs, and the run lengths cover the text exactly.
        let runs = vec![
            test_text_run(0, 2, "ab", false),
            test_text_run(2, 2, "CD", true),
        ];
        let (lines, leftover) = combine_terminal_row_runs(runs, test_gap_font(), test_gap_color());
        assert!(leftover.is_empty(), "both spans should combine");
        let line = lines.into_iter().next().expect("simple row combines");
        assert_eq!(line.text, "abCD");
        assert_eq!(line.start_col, 0);
        assert_eq!(line.runs.len(), 2, "two style runs, one shaped line");
        assert_eq!(
            line.runs.iter().map(|run| run.len).sum::<usize>(),
            line.text.len(),
            "run lengths must cover the line text exactly"
        );
    }

    #[test]
    fn combine_fills_column_gaps_with_spaces() {
        // A gap between spans is filled with spaces so the later span stays on
        // its grid column.
        let runs = vec![
            test_text_run(0, 2, "ab", false),
            test_text_run(5, 2, "CD", false),
        ];
        let (lines, leftover) = combine_terminal_row_runs(runs, test_gap_font(), test_gap_color());
        assert!(leftover.is_empty(), "both spans should combine");
        let line = lines
            .into_iter()
            .next()
            .expect("simple row with a gap combines");
        assert_eq!(line.text, "ab   CD"); // 3-column gap -> 3 spaces
        assert_eq!(
            line.runs.iter().map(|run| run.len).sum::<usize>(),
            line.text.len()
        );
        assert_eq!(line.runs.len(), 3, "span, gap, span");
    }

    #[test]
    fn combine_falls_back_for_wide_cells() {
        // A wide (CJK) cell occupies 2 columns with 1 codepoint; combining would
        // re-flow by natural advance and break grid alignment, so it must keep
        // the per-span path.
        let runs = vec![test_text_run(0, 2, "中", false)];
        let (lines, leftover) = combine_terminal_row_runs(runs, test_gap_font(), test_gap_color());
        assert!(lines.is_empty());
        assert_eq!(leftover.len(), 1);
    }

    #[test]
    fn combine_falls_back_for_multi_codepoint_cells() {
        // Defensive: a cell whose text is more than one char for one column
        // (e.g. a grapheme cluster) is not a 1:1 char/cell mapping.
        let runs = vec![test_text_run(0, 1, "a\u{0301}", false)]; // "á" as a + combining accent
        let (lines, leftover) = combine_terminal_row_runs(runs, test_gap_font(), test_gap_color());
        assert!(lines.is_empty());
        assert_eq!(leftover.len(), 1);
    }

    #[test]
    fn combining_mark_cell_stays_in_one_paint_segment() {
        // A cell whose text is a base char plus a combining mark (e.g. "á" as
        // "a" + U+0301) must be shaped and painted as one unit at its own
        // column, not split per Rust `char` — otherwise the mark would be
        // displaced into the next column instead of stacking on the base.
        let run = test_text_run(3, 1, "a\u{0301}", false);
        assert_eq!(run.segments.len(), 1, "one cell, one paint segment");
        assert_eq!(run.segments[0].col, 3);
        assert_eq!(
            &run.text[run.segments[0].byte_start..][..run.segments[0].byte_len],
            "a\u{0301}"
        );
    }

    #[test]
    fn merged_run_keeps_one_segment_per_originating_cell() {
        // Simple (1:1 char-to-cell) ASCII cells can be merged into one run's
        // `text` buffer, but each originating cell keeps its own segment so a
        // later non-ASCII cell appended to the same run (e.g. after further
        // upstream changes) would still be positioned by cell, not by char.
        let mut run = test_text_run(0, 1, "a", false);
        run.append_text("b", 1);
        run.append_text("c", 1);
        assert_eq!(run.text, "abc");
        assert_eq!(run.segments.len(), 3);
        assert_eq!(
            run.segments.iter().map(|s| s.col).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn combine_keeps_combining_around_a_noncombinable_span() {
        // A wide/non-combinable span in the middle of a row must not block the
        // ASCII spans before and after it from still combining into their own
        // shaped lines.
        let runs = vec![
            test_text_run(0, 2, "ab", false),
            test_text_run(2, 2, "CD", false),
            test_text_run(4, 2, "中", false),
            test_text_run(6, 2, "ef", false),
            test_text_run(8, 2, "GH", false),
        ];
        let (lines, leftover) = combine_terminal_row_runs(runs, test_gap_font(), test_gap_color());
        assert_eq!(leftover.len(), 1, "only the wide span stays per-span");
        assert_eq!(leftover[0].text, "中");
        assert_eq!(
            lines.len(),
            2,
            "spans before and after the wide span each combine"
        );
        assert_eq!(lines[0].text, "abCD");
        assert_eq!(lines[1].text, "efGH");
    }

    #[test]
    fn precomputed_row_hash_matches_per_frame_hash() {
        // The renderer keys its row cache by the precomputed hash; it must equal
        // the value `terminal_row_hash` would compute for the same contiguous
        // row slice, otherwise cache entries would never be shared/found.
        let line0 = vec![test_indexed_cell(0, 0, "h"), test_indexed_cell(0, 1, "i")];
        let line1 = vec![test_indexed_cell(1, 0, "y"), test_indexed_cell(1, 1, "o")];
        let mut all = line0.clone();
        all.extend(line1.clone());

        let precomputed = PrecomputedRowHashes::from_cells(&all);
        assert_eq!(precomputed.get(0), Some(terminal_row_hash(&line0)));
        assert_eq!(precomputed.get(1), Some(terminal_row_hash(&line1)));
        assert_eq!(precomputed.get(2), None, "absent lines have no hash");
    }

    #[test]
    fn precomputed_row_hash_changes_with_content() {
        let before = vec![test_indexed_cell(0, 0, "a")];
        let after = vec![test_indexed_cell(0, 0, "b")];
        assert_ne!(
            PrecomputedRowHashes::from_cells(&before).get(0),
            PrecomputedRowHashes::from_cells(&after).get(0),
        );
    }

    #[test]
    fn row_hash_ignores_screen_row_for_scroll_reuse() {
        let before = vec![test_indexed_cell(0, 0, "a")];
        let mut after = vec![test_indexed_cell(1, 0, "a")];
        after[0].cell.row = 1;

        assert_eq!(terminal_row_hash(&before), terminal_row_hash(&after));
    }

    #[test]
    fn unchanged_rows_keep_versions() {
        let mut tracker = RowVersionTracker::default();
        let rows = [(0i32, 10u64), (1, 20), (2, 30)];
        let first = tracker.assign(&rows);
        let second = tracker.assign(&rows);
        assert_eq!(first, second, "stable content keeps versions");
        assert_eq!(
            first.iter().collect::<std::collections::HashSet<_>>().len(),
            3,
            "distinct lines get distinct versions"
        );
    }

    #[test]
    fn changed_row_bumps_only_itself() {
        let mut tracker = RowVersionTracker::default();
        let first = tracker.assign(&[(0, 10), (1, 20), (2, 30)]);
        let second = tracker.assign(&[(0, 10), (1, 99), (2, 30)]); // line 1 changed
        assert_eq!(first[0], second[0]);
        assert_ne!(first[1], second[1]);
        assert_eq!(first[2], second[2]);
    }

    #[test]
    fn scroll_keeps_versions_for_retained_lines() {
        // Streaming output: lines 0..=2 are visible, then a new line 3 is written
        // and the viewport shows lines 1..=3 (line 0 scrolled out). Retained
        // lines 1 and 2 keep their versions; the new line 3 gets a fresh one.
        let mut tracker = RowVersionTracker::default();
        let first = tracker.assign(&[(0, 10), (1, 20), (2, 30)]);
        let second = tracker.assign(&[(1, 20), (2, 30), (3, 40)]);
        assert_eq!(first[1], second[0], "line 1 keeps its version after scroll");
        assert_eq!(first[2], second[1], "line 2 keeps its version after scroll");
        assert!(
            !first.contains(&second[2]),
            "the newly exposed line gets a version not seen before"
        );
    }

    #[test]
    fn reused_line_number_with_new_content_gets_fresh_version() {
        // After a clear/resize the same absolute line numbers carry different
        // content, so they must get fresh versions.
        let mut tracker = RowVersionTracker::default();
        let first = tracker.assign(&[(0, 10), (1, 20)]);
        let second = tracker.assign(&[(0, 11), (1, 21)]);
        assert_ne!(first[0], second[0]);
        assert_ne!(first[1], second[1]);
    }

    #[test]
    fn new_rows_get_monotonic_versions() {
        let mut tracker = RowVersionTracker::default();
        let first = tracker.assign(&[(0, 10)]);
        let second = tracker.assign(&[(0, 10), (1, 20)]); // line 1 new
        let third = tracker.assign(&[(0, 10), (1, 20), (2, 30)]); // line 2 new
        assert_eq!(first[0], second[0]);
        assert_eq!(second[1], third[1]);
        assert!(third[2] > second[1] && second[1] > first[0]);
    }

    #[test]
    fn a_to_b_then_back_to_a_distinguishes_generations() {
        // A line that changes away and back is a genuinely new render: the
        // version must not silently alias the original, otherwise a stale
        // prepared row could be reused.
        let mut tracker = RowVersionTracker::default();
        let a1 = tracker.assign(&[(0, 10)]);
        let _b = tracker.assign(&[(0, 20)]);
        let a2 = tracker.assign(&[(0, 10)]);
        assert_ne!(a1[0], a2[0], "content A after B is a fresh version");
    }
}
