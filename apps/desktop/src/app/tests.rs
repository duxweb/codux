#[cfg(test)]
mod tests {
    use crate::{
        app::{
            ai_history_mapping::{
                AI_SESSION_FORK_TARGETS, ai_session_fork_command, ai_session_restore_command,
            },
            app_helpers::{
                file_search_status_message, generated_git_commit_message, git_remote_action_label,
                project_badge_text_from_name, ssh_connect_command,
            },
            shortcuts::{normalized_shortcut_text, shortcut_matches},
            terminal_state::{
                TerminalSplitDirection, normalize_terminal_restore_state,
                structural_terminal_layout, terminal_pane_terminal_id, terminal_restore_plan,
                terminal_restore_plan_for_language, terminal_split_tree_insert_pane,
                terminal_split_tree_insert_pane_root, terminal_split_tree_remove_pane,
                terminal_split_tree_update_ratios,
            },
            terminal_worktree_actions::active_terminal_slot_indices,
            terminal_worktree_actions::restored_live_active_terminal_id,
            types::{TerminalPanePlan, TerminalTabPlan},
            ui_helpers::restored_terminal_preview_lines,
            workspace_views::{terminal_pane_drop_target_at_position, terminal_pane_rect},
        },
        terminal::TerminalLaunchContext,
    };
    use codux_runtime::{
        ai_history::{AISessionForkTarget, AISessionSummary},
        git::GitSummary,
        ssh::SSHProfileSummary,
        terminal_layout::{
            SplitAxis, TerminalGridColumn, TerminalLayoutSummary, TerminalPaneSummary,
            TerminalSplitNode, TerminalTabSummary, TerminalTopGrid,
        },
        terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary},
    };
    use gpui::{Bounds, point, px, size};
    use std::{collections::HashMap, path::PathBuf};

    fn terminal_focus_test_tabs() -> Vec<crate::app::types::TerminalTab> {
        vec![crate::app::types::TerminalTab {
            id: 1,
            label: "Main".to_string(),
            terminal_id: None,
            panes: vec![
                crate::app::types::TerminalPaneSlot {
                    title: "Split 1".to_string(),
                    terminal_id: Some("top-1".to_string()),
                    pane: None,
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                },
                crate::app::types::TerminalPaneSlot {
                    title: "Split 2".to_string(),
                    terminal_id: Some("top-2".to_string()),
                    pane: None,
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                },
            ],
        }]
    }

    #[test]
    fn terminal_restore_plan_migrates_legacy_tabs_to_top_panes() {
        let layout = TerminalLayoutSummary {
            active_terminal_id: "term-d".to_string(),
            top_panes: vec![
                TerminalPaneSummary {
                    title: "分屏 1".to_string(),
                    terminal_id: "term-a".to_string(),
                },
                TerminalPaneSummary {
                    title: "长任务".to_string(),
                    terminal_id: "term-b".to_string(),
                },
            ],
            tabs: vec![
                TerminalTabSummary {
                    label: "标签页 1".to_string(),
                    terminal_id: "term-c".to_string(),
                },
                TerminalTabSummary {
                    label: "标签页 2".to_string(),
                    terminal_id: "term-d".to_string(),
                },
            ],
            top_ratios: vec![0.5, 0.5],
            top_grid: TerminalTopGrid::default(),
            split_tree: None,
            bottom_ratio: 0.32,
            collapsed_panes: Vec::new(),
            error: None,
        };

        let runtime = TerminalRuntimeSummary {
            sessions: vec![TerminalRuntimeSessionSummary {
                terminal_id: "term-a".to_string(),
                title: "分屏 1".to_string(),
                project_id: "project-1".to_string(),
                project_name: "Codux".to_string(),
                project_path: "/workspace/codux".to_string(),
                cwd: "/workspace/codux".to_string(),
                status: "running".to_string(),
                is_running: true,
                created_at: 1.0,
                last_active_at: 2.0,
                has_buffer: false,
                buffer_characters: 0,
                input_bytes: 0,
                last_input_at: None,
                input_history: Vec::new(),
                output_bytes: 10,
                output_tail: "restored top output".to_string(),
            }],
            ..Default::default()
        };
        let plan = terminal_restore_plan_for_language(
            &layout,
            &runtime,
            "simplifiedChinese",
            Some("term-d".to_string()),
        );

        assert_eq!(plan.tabs.len(), 1);
        assert_eq!(
            plan.tabs[0]
                .panes
                .iter()
                .map(|pane| pane.title.as_str())
                .collect::<Vec<_>>(),
            vec!["分屏 1", "长任务", "标签页 1", "标签页 2"]
        );
        assert_eq!(plan.tabs[0].panes[0].terminal_id.as_deref(), Some("term-a"));
        assert_eq!(
            plan.tabs[0].panes[0].restored_output_tail,
            "restored top output"
        );
        assert_eq!(plan.active_index, 0);
        assert_eq!(plan.active_terminal_id.as_deref(), Some("term-d"));
    }

    #[test]
    fn terminal_restore_state_migrates_legacy_bottom_tabs() {
        let layout = TerminalLayoutSummary {
            active_terminal_id: "gpui-term-worktree-1-bottom-1".to_string(),
            top_panes: vec![TerminalPaneSummary {
                title: "分屏 1".to_string(),
                terminal_id: "gpui-term-worktree-1-top-1".to_string(),
            }],
            tabs: vec![TerminalTabSummary {
                label: "标签页 1".to_string(),
                terminal_id: "gpui-term-worktree-1-bottom-1".to_string(),
            }],
            top_ratios: vec![1.0],
            top_grid: TerminalTopGrid::default(),
            split_tree: None,
            bottom_ratio: 0.32,
            collapsed_panes: Vec::new(),
            error: None,
        };
        let runtime = TerminalRuntimeSummary {
            active_terminal_id: "gpui-term-worktree-1-top-1".to_string(),
            sessions: vec![TerminalRuntimeSessionSummary {
                terminal_id: "gpui-term-worktree-1-top-1".to_string(),
                title: "分屏 1".to_string(),
                project_id: "worktree-1".to_string(),
                project_name: "Codux".to_string(),
                project_path: "/workspace/codux".to_string(),
                cwd: "/workspace/codux".to_string(),
                status: "running".to_string(),
                is_running: true,
                created_at: 1.0,
                last_active_at: 2.0,
                has_buffer: false,
                buffer_characters: 0,
                input_bytes: 0,
                last_input_at: None,
                input_history: Vec::new(),
                output_bytes: 12,
                output_tail: "worktree top output".to_string(),
            }],
            ..Default::default()
        };

        let (layout, runtime) = normalize_terminal_restore_state(
            Some("worktree-1"),
            layout,
            runtime,
            "simplifiedChinese",
        );
        let plan = terminal_restore_plan_for_language(
            &layout,
            &runtime,
            "simplifiedChinese",
            Some("gpui-term-worktree-1-bottom-1".to_string()),
        );

        assert_eq!(layout.active_terminal_id, "");
        assert_eq!(
            layout.top_panes[0].terminal_id,
            "gpui-term-worktree-1-top-1"
        );
        assert!(layout.tabs.is_empty());
        assert_eq!(
            layout.top_panes[1].terminal_id,
            "gpui-term-worktree-1-bottom-1"
        );
        assert_eq!(runtime.sessions.len(), 1);
        assert_eq!(plan.active_index, 0);
        assert_eq!(
            plan.active_terminal_id.as_deref(),
            Some("gpui-term-worktree-1-bottom-1")
        );
        assert_eq!(
            plan.tabs[0].panes[0].restored_output_tail,
            "worktree top output"
        );
    }

    #[test]
    fn terminal_restore_state_rebuilds_invalid_layout_without_compat_fallback() {
        let layout = TerminalLayoutSummary {
            active_terminal_id: String::new(),
            top_panes: vec![TerminalPaneSummary {
                title: "旧布局".to_string(),
                terminal_id: String::new(),
            }],
            tabs: Vec::new(),
            top_ratios: vec![1.0],
            top_grid: TerminalTopGrid::default(),
            split_tree: None,
            bottom_ratio: 0.32,
            collapsed_panes: Vec::new(),
            error: None,
        };

        let (layout, _) = normalize_terminal_restore_state(
            Some("worktree-1"),
            layout,
            TerminalRuntimeSummary::default(),
            "simplifiedChinese",
        );

        assert_eq!(layout.top_panes.len(), 1);
        assert!(layout.tabs.is_empty());
        assert_eq!(layout.top_panes[0].title, "终端 1");
        assert_eq!(layout.active_terminal_id, "");
    }

    #[test]
    fn terminal_restore_plan_falls_back_to_single_terminal() {
        let plan = terminal_restore_plan(
            &TerminalLayoutSummary::default(),
            &TerminalRuntimeSummary::default(),
        );

        assert_eq!(plan.active_index, 0);
        assert_eq!(
            plan.tabs,
            vec![TerminalTabPlan {
                terminal_id: None,
                label: "终端 1".to_string(),
                panes: vec![TerminalPanePlan {
                    terminal_id: None,
                    title: "终端 1".to_string(),
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                }],
            }]
        );
    }

    #[test]
    fn restored_terminal_preview_lines_use_last_non_empty_rows() {
        assert_eq!(
            restored_terminal_preview_lines("one\n\ntwo\nthree\nfour\nfive\n"),
            vec!["two", "three", "four", "five"]
        );
        assert_eq!(
            restored_terminal_preview_lines(&"x".repeat(120)),
            vec!["x".repeat(96)]
        );
    }

    #[test]
    fn terminal_pane_terminal_id_normalizes_existing_runtime_id() {
        let base = TerminalLaunchContext {
            root_project_id: "project-1".to_string(),
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: PathBuf::from("/runtime-root"),
            terminal_id: None,
            slot_id: None,
            session_key: None,
            session_title: None,
            session_cwd: None,
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
            host_device_id: None,
        };

        let pane = TerminalPanePlan {
            terminal_id: Some("term-existing".to_string()),
            title: "分屏 2".to_string(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };

        let terminal_id =
            terminal_pane_terminal_id(Some(&base), &pane).expect("terminal id should be derived");
        let repeated =
            terminal_pane_terminal_id(Some(&base), &pane).expect("terminal id should be derived");

        assert_eq!(terminal_id, "term-existing");
        assert_eq!(terminal_id, repeated);
    }

    #[test]
    fn terminal_pane_terminal_id_rejects_foreign_owner_id() {
        // A pane carrying ANOTHER workspace's terminal id (leaked during a laggy
        // project switch) must NOT be re-prefixed into this owner: that accreted
        // a project segment on every switch and could resolve to the other
        // project's live pane (cross-talk). It gets a fresh id owned by THIS
        // project instead; an id already owned by this project is kept as-is.
        let base = TerminalLaunchContext {
            root_project_id: "project-1".to_string(),
            project_id: "project-B".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: PathBuf::from("/runtime-root"),
            terminal_id: None,
            slot_id: None,
            session_key: None,
            session_title: None,
            session_cwd: None,
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
            host_device_id: None,
        };

        let foreign = TerminalPanePlan {
            terminal_id: Some("gpui-term-project-A-1234".to_string()),
            title: "分屏 2".to_string(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let derived = terminal_pane_terminal_id(Some(&base), &foreign)
            .expect("terminal id should be derived");
        assert!(
            derived.starts_with("gpui-term-project-B-"),
            "foreign id not re-owned to this project: {derived}"
        );
        assert!(
            !derived.contains("project-A"),
            "foreign project segment leaked into id: {derived}"
        );

        let owned = TerminalPanePlan {
            terminal_id: Some("gpui-term-project-B-abc".to_string()),
            title: "分屏 1".to_string(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        assert_eq!(
            terminal_pane_terminal_id(Some(&base), &owned).expect("terminal id should be derived"),
            "gpui-term-project-B-abc"
        );
    }

    #[test]
    fn structural_terminal_layout_removes_entries_without_terminal_ids() {
        let layout = TerminalLayoutSummary {
            active_terminal_id: "term-2".to_string(),
            top_panes: vec![
                TerminalPaneSummary {
                    title: "missing".to_string(),
                    terminal_id: String::new(),
                },
                TerminalPaneSummary {
                    title: "kept".to_string(),
                    terminal_id: "term-2".to_string(),
                },
            ],
            tabs: vec![TerminalTabSummary {
                label: "tab".to_string(),
                terminal_id: String::new(),
            }],
            top_ratios: vec![0.5, 0.5],
            top_grid: TerminalTopGrid {
                columns: vec![TerminalGridColumn {
                    ratio: 1.0,
                    rows: 2,
                    row_ratios: vec![0.25, 0.75],
                }],
            },
            split_tree: None,
            bottom_ratio: 0.32,
            collapsed_panes: Vec::new(),
            error: None,
        };

        let layout = structural_terminal_layout(layout);

        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].title, "kept");
        assert_eq!(layout.top_grid.columns.len(), 1);
        assert_eq!(layout.top_grid.columns[0].rows, 1);
        assert!(layout.tabs.is_empty());
        assert_eq!(layout.active_terminal_id, "");
    }

    #[test]
    fn terminal_split_tree_supports_nested_splits_and_ratio_updates() {
        let root = TerminalSplitNode::Leaf { pane: 0 };
        let down =
            terminal_split_tree_insert_pane(&root, 0, 1, TerminalSplitDirection::Down).unwrap();
        let nested =
            terminal_split_tree_insert_pane(&down, 0, 1, TerminalSplitDirection::Right).unwrap();

        assert_eq!(
            nested,
            TerminalSplitNode::Split {
                axis: SplitAxis::Vertical,
                ratios: vec![0.5, 0.5],
                children: vec![
                    TerminalSplitNode::Split {
                        axis: SplitAxis::Horizontal,
                        ratios: vec![0.5, 0.5],
                        children: vec![
                            TerminalSplitNode::Leaf { pane: 0 },
                            TerminalSplitNode::Leaf { pane: 1 },
                        ],
                    },
                    TerminalSplitNode::Leaf { pane: 2 },
                ],
            }
        );

        let updated = terminal_split_tree_update_ratios(&nested, &[0], vec![1.0, 3.0]);
        match updated {
            TerminalSplitNode::Split { children, .. } => match &children[0] {
                TerminalSplitNode::Split { ratios, .. } => {
                    assert_eq!(ratios, &vec![0.25, 0.75]);
                }
                _ => panic!("expected nested horizontal split"),
            },
            _ => panic!("expected root split"),
        }
    }

    #[test]
    fn terminal_split_tree_rebalances_after_insert_and_remove() {
        let tree = TerminalSplitNode::Split {
            axis: SplitAxis::Horizontal,
            ratios: vec![0.25, 0.75],
            children: vec![
                TerminalSplitNode::Leaf { pane: 0 },
                TerminalSplitNode::Leaf { pane: 1 },
            ],
        };

        let inserted =
            terminal_split_tree_insert_pane(&tree, 0, 1, TerminalSplitDirection::Right).unwrap();
        assert_eq!(
            inserted,
            TerminalSplitNode::Split {
                axis: SplitAxis::Horizontal,
                ratios: vec![1.0 / 3.0; 3],
                children: vec![
                    TerminalSplitNode::Leaf { pane: 0 },
                    TerminalSplitNode::Leaf { pane: 1 },
                    TerminalSplitNode::Leaf { pane: 2 },
                ],
            }
        );

        let removed = terminal_split_tree_remove_pane(&inserted, 1).unwrap();
        assert_eq!(
            removed,
            TerminalSplitNode::Split {
                axis: SplitAxis::Horizontal,
                ratios: vec![0.5, 0.5],
                children: vec![
                    TerminalSplitNode::Leaf { pane: 0 },
                    TerminalSplitNode::Leaf { pane: 1 },
                ],
            }
        );
    }

    #[test]
    fn terminal_split_tree_root_insert_wraps_or_extends_root() {
        let nested = TerminalSplitNode::Split {
            axis: SplitAxis::Vertical,
            ratios: vec![0.5, 0.5],
            children: vec![
                TerminalSplitNode::Split {
                    axis: SplitAxis::Horizontal,
                    ratios: vec![0.5, 0.5],
                    children: vec![
                        TerminalSplitNode::Leaf { pane: 0 },
                        TerminalSplitNode::Leaf { pane: 1 },
                    ],
                },
                TerminalSplitNode::Leaf { pane: 2 },
            ],
        };

        let right = terminal_split_tree_insert_pane_root(&nested, 3, TerminalSplitDirection::Right)
            .unwrap();
        assert_eq!(
            right,
            TerminalSplitNode::Split {
                axis: SplitAxis::Horizontal,
                ratios: vec![0.5, 0.5],
                children: vec![nested.clone(), TerminalSplitNode::Leaf { pane: 3 }],
            }
        );

        let up =
            terminal_split_tree_insert_pane_root(&nested, 0, TerminalSplitDirection::Up).unwrap();
        assert_eq!(
            up,
            TerminalSplitNode::Split {
                axis: SplitAxis::Vertical,
                ratios: vec![1.0 / 3.0; 3],
                children: vec![
                    TerminalSplitNode::Leaf { pane: 0 },
                    TerminalSplitNode::Split {
                        axis: SplitAxis::Horizontal,
                        ratios: vec![0.5, 0.5],
                        children: vec![
                            TerminalSplitNode::Leaf { pane: 1 },
                            TerminalSplitNode::Leaf { pane: 2 },
                        ],
                    },
                    TerminalSplitNode::Leaf { pane: 3 },
                ],
            }
        );
    }

    #[test]
    fn terminal_grid_drag_hit_test_maps_columns_and_rows() {
        let split_tree = TerminalSplitNode::Split {
            axis: SplitAxis::Horizontal,
            ratios: vec![0.5, 0.5],
            children: vec![
                TerminalSplitNode::Leaf { pane: 0 },
                TerminalSplitNode::Split {
                    axis: SplitAxis::Vertical,
                    ratios: vec![0.5, 0.5],
                    children: vec![
                        TerminalSplitNode::Leaf { pane: 1 },
                        TerminalSplitNode::Leaf { pane: 2 },
                    ],
                },
            ],
        };
        let bounds = Bounds::new(point(px(10.0), px(20.0)), size(px(400.0), px(300.0)));

        assert_eq!(
            terminal_pane_drop_target_at_position(
                &split_tree,
                3,
                bounds,
                point(px(40.0), px(120.0))
            ),
            Some(0),
        );
        assert_eq!(
            terminal_pane_drop_target_at_position(
                &split_tree,
                3,
                bounds,
                point(px(260.0), px(80.0))
            ),
            Some(1),
        );
        assert_eq!(
            terminal_pane_drop_target_at_position(
                &split_tree,
                3,
                bounds,
                point(px(260.0), px(260.0))
            ),
            Some(2),
        );

        let bottom_right = terminal_pane_rect(&split_tree, 3, 2);
        assert!((bottom_right.left - 0.5).abs() < 0.001);
        assert!((bottom_right.top - 0.5).abs() < 0.001);
        assert!((bottom_right.width - 0.5).abs() < 0.001);
        assert!((bottom_right.height - 0.5).abs() < 0.001);
    }

    #[test]
    fn active_terminal_slot_indices_use_layout_terminal_id_not_last_pane() {
        let terminals = terminal_focus_test_tabs();

        assert_eq!(
            active_terminal_slot_indices(&terminals, "top-1", 1),
            Some((0, 0))
        );
        assert_eq!(
            active_terminal_slot_indices(&terminals, "top-2", 1),
            Some((0, 1))
        );
        assert_eq!(
            active_terminal_slot_indices(&terminals, "bottom-1", 1),
            Some((0, 0))
        );
        assert_eq!(
            active_terminal_slot_indices(&terminals, "", 1),
            Some((0, 0))
        );
        assert_eq!(
            active_terminal_slot_indices(&terminals, "top-1", 1),
            Some((0, 0)),
            "terminal focus should resolve by pane runtime id"
        );
    }

    #[test]
    fn restored_live_active_terminal_id_preserves_focused_top_pane() {
        let terminals = terminal_focus_test_tabs();

        assert_eq!(
            restored_live_active_terminal_id(&terminals, "top-2", Some("top-1")).as_deref(),
            Some("top-2")
        );
        assert_eq!(
            restored_live_active_terminal_id(&terminals, "missing", Some("top-1")).as_deref(),
            Some("top-1")
        );
        assert_eq!(
            restored_live_active_terminal_id(&terminals, "missing", Some("gone")),
            None
        );
    }

    #[test]
    fn ai_session_restore_command_matches_tauri_history_restore() {
        let mut session = AISessionSummary {
            id: "local-id".to_string(),
            session_key: "session key".to_string(),
            external_session_id: Some("external-1".to_string()),
            title: "Task".to_string(),
            source: "codex".to_string(),
            project_name: None,
            project_path: None,
            last_model: None,
            last_seen_at: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
            active_duration_seconds: 0,
            usage_amounts: Vec::new(),
        };

        assert_eq!(
            ai_session_restore_command(&session),
            "codex resume external-1"
        );

        session.source = "claude-code".to_string();
        assert_eq!(
            ai_session_restore_command(&session),
            "claude --resume external-1"
        );

        session.source = "opencode".to_string();
        session.external_session_id = None;
        assert_eq!(
            ai_session_restore_command(&session),
            "opencode --session 'session key'"
        );

        session.source = "mimo".to_string();
        assert_eq!(
            ai_session_restore_command(&session),
            "mimo --session 'session key'"
        );

        session.source = "kiro".to_string();
        session.external_session_id = Some("session-1".to_string());
        assert_eq!(
            ai_session_restore_command(&session),
            "kiro-cli --resume-id session-1"
        );

        session.source = "antigravity".to_string();
        session.external_session_id = None;
        assert_eq!(
            ai_session_restore_command(&session),
            "agy resume 'session key'"
        );

        session.source = "codewhale".to_string();
        session.external_session_id = Some("external-2".to_string());
        assert_eq!(
            ai_session_restore_command(&session),
            "codewhale resume external-2"
        );
    }

    #[test]
    fn ai_session_fork_command_reads_prompt_file_for_all_targets() {
        let path = "/tmp/codux session handoff.md";
        for target in AI_SESSION_FORK_TARGETS {
            let command = ai_session_fork_command(target, path);
            if cfg!(windows) {
                assert!(
                    command
                        .contains("Get-Content -Raw -LiteralPath '/tmp/codux session handoff.md'"),
                    "{command}"
                );
            } else {
                assert!(
                    command.contains("$(cat '/tmp/codux session handoff.md')"),
                    "{command}"
                );
            }
            assert!(!command.contains("Continue Cleaned AI Session"));
        }
        assert!(ai_session_fork_command(AISessionForkTarget::Codex, path).starts_with("codex "));
        assert!(ai_session_fork_command(AISessionForkTarget::Claude, path).starts_with("claude "));
        assert!(ai_session_fork_command(AISessionForkTarget::Agy, path).starts_with("agy "));
        assert!(
            ai_session_fork_command(AISessionForkTarget::OpenCode, path)
                .starts_with("opencode run ")
        );
        assert!(ai_session_fork_command(AISessionForkTarget::Kiro, path).starts_with("kiro-cli "));
        assert!(
            ai_session_fork_command(AISessionForkTarget::CodeWhale, path).starts_with("codewhale ")
        );
        assert!(ai_session_fork_command(AISessionForkTarget::Kimi, path).starts_with("kimi "));
        assert!(ai_session_fork_command(AISessionForkTarget::MiMo, path).starts_with("mimo run "));
    }

    #[test]
    fn ssh_connect_command_uses_saved_profile_id_without_exposing_endpoint() {
        let profile = SSHProfileSummary {
            id: "profile with spaces".to_string(),
            name: "Production".to_string(),
            endpoint: "root@example.com:22".to_string(),
            credential_kind: "password".to_string(),
            updated_at: 123,
        };

        assert_eq!(
            ssh_connect_command(&profile),
            "codux-ssh 'profile with spaces'"
        );
    }

    #[test]
    fn generated_git_commit_message_prefers_staged_count() {
        let git = GitSummary {
            staged: 1,
            unstaged: 3,
            untracked: 2,
            ..Default::default()
        };
        assert_eq!(generated_git_commit_message(&git), "Update 1 staged file");

        let git = GitSummary {
            staged: 0,
            unstaged: 2,
            untracked: 1,
            ..Default::default()
        };
        assert_eq!(generated_git_commit_message(&git), "Update 3 changed files");

        assert_eq!(
            generated_git_commit_message(&GitSummary::default()),
            "Update project files"
        );
    }

    #[test]
    fn project_badge_text_uses_up_to_four_project_name_chars() {
        assert_eq!(
            project_badge_text_from_name(" Codux GPUI "),
            Some("CG".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("getUserInfo"),
            Some("GUI".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("wx-pay-api"),
            Some("WPA".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("codux"),
            Some("CODU".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("项目"),
            Some("项目".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("用户中心"),
            Some("用户中心".to_string())
        );
        assert_eq!(project_badge_text_from_name("  "), None);
    }

    #[test]
    fn git_remote_action_label_names_remote_pushes() {
        assert_eq!(git_remote_action_label("fetch"), "fetch");
        assert_eq!(git_remote_action_label("push:origin"), "push to origin");
    }

    #[test]
    fn shortcut_text_normalizes_tauri_display_formats() {
        assert_eq!(
            normalized_shortcut_text("Cmd+Shift+P"),
            Some("Meta+Shift+P".to_string())
        );
        assert_eq!(
            normalized_shortcut_text("⌘⇧P"),
            Some("Meta+Shift+P".to_string())
        );
        assert_eq!(
            normalized_shortcut_text("Control+Alt+Delete"),
            Some("Ctrl+Alt+delete".to_string())
        );
    }

    #[test]
    fn shortcut_matching_uses_custom_value_or_default() {
        let mut shortcuts = HashMap::new();
        shortcuts.insert("view.files".to_string(), "Cmd+Shift+F / Ctrl+F".to_string());
        assert!(shortcut_matches(&shortcuts, "view.files", "⌘⇧F"));
        assert!(shortcut_matches(&shortcuts, "view.files", "⌃F"));
        assert!(!shortcut_matches(&shortcuts, "view.files", "⌘2"));

        shortcuts.clear();
        let default_terminal = if cfg!(target_os = "macos") {
            "⌘⌥1"
        } else {
            "Ctrl+Alt+1"
        };
        assert!(shortcut_matches(
            &shortcuts,
            "view.terminal",
            default_terminal
        ));
        let project_switch = if cfg!(target_os = "macos") {
            "⌘1"
        } else {
            "Ctrl+1"
        };
        assert!(!shortcut_matches(
            &shortcuts,
            "view.terminal",
            project_switch
        ));
        let default_project = if cfg!(target_os = "macos") {
            "⌘N"
        } else {
            "Ctrl+N"
        };
        let default_task = if cfg!(target_os = "macos") {
            "⌘⇧N"
        } else {
            "Ctrl+Shift+N"
        };
        assert!(shortcut_matches(&shortcuts, "task.create", default_task));
        assert!(shortcut_matches(
            &shortcuts,
            "project.create",
            default_project
        ));

        let default_git_panel = if cfg!(target_os = "macos") {
            "⌘⇧G"
        } else {
            "Ctrl+Shift+G"
        };
        assert!(shortcut_matches(
            &shortcuts,
            "assistant.git.open",
            default_git_panel
        ));
        assert!(shortcut_matches(&shortcuts, "panel.git", default_git_panel));
        let default_terminal_split = if cfg!(target_os = "macos") {
            "⌘T"
        } else {
            "Ctrl+T"
        };
        assert!(shortcut_matches(
            &shortcuts,
            "terminal.split",
            default_terminal_split
        ));
        let default_projects_sidebar = if cfg!(target_os = "macos") {
            "⌘⌥P"
        } else {
            "Ctrl+Alt+P"
        };
        assert!(shortcut_matches(
            &shortcuts,
            "sidebar.projects.toggle",
            default_projects_sidebar
        ));
    }

    #[test]
    fn file_search_status_message_reports_match_position() {
        assert_eq!(
            file_search_status_message(0, 0),
            "file search has no matches"
        );
        assert_eq!(file_search_status_message(1, 3), "file search match 2 of 3");
    }
}
