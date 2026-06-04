#[cfg(test)]
mod tests {
    use crate::{
        app::{
            ai_history_mapping::ai_session_restore_command,
            app_helpers::{
                file_search_status_message, generated_git_commit_message, git_remote_action_label,
                project_badge_text_from_name, ssh_connect_command,
            },
            app_state::{
                empty_worktree_view_state, initial_project_view_store, initial_worktree_view_store,
            },
            shortcuts::{normalized_shortcut_text, shortcut_matches},
            terminal_state::{
                normalize_terminal_restore_state, structural_terminal_layout,
                terminal_pane_terminal_id, terminal_restore_plan,
            },
            types::{TerminalPanePlan, TerminalTabPlacement, TerminalTabPlan},
            ui_helpers::restored_terminal_preview_lines,
        },
        terminal::TerminalLaunchContext,
    };
    use codux_runtime::terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary};
    use codux_runtime::{
        ai_history::{AIHistorySummary, AISessionSummary},
        git::GitSummary,
        runtime_state::RuntimeState,
        ssh::SSHProfileSummary,
        terminal_layout::{
            TerminalLayoutService, TerminalLayoutSummary, TerminalPaneSummary, TerminalTabSummary,
            terminal_layout_storage_key,
        },
    };
    use std::{
        collections::HashMap,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn initial_project_view_store_preloads_all_project_worktrees_from_state_json() {
        let support_dir = temp_support_dir("project-view-store");
        fs::create_dir_all(&support_dir).unwrap();
        fs::write(
            support_dir.join("state.json"),
            r#"{
                "projects": [
                    {"id": "project-a", "name": "A", "path": "/tmp/a"},
                    {"id": "project-b", "name": "B", "path": "/tmp/b"}
                ],
                "selectedProjectId": "project-a",
                "worktrees": [
                    {"id": "project-a", "projectId": "project-a", "name": "main", "branch": "main", "path": "/tmp/a", "status": "todo", "isDefault": true},
                    {"id": "task-b", "projectId": "project-b", "name": "Task B", "branch": "feature/b", "path": "/tmp/b-task", "status": "todo", "isDefault": false}
                ],
                "worktreeTasks": [
                    {"worktreeId": "task-b", "title": "Persisted task B", "baseBranch": "main", "baseCommit": null, "status": "todo", "createdAt": 1, "updatedAt": 2, "startedAt": null, "completedAt": null}
                ],
                "selectedWorktreeIdByProject": {
                    "project-a": "project-a",
                    "project-b": "task-b"
                }
            }"#,
        )
        .unwrap();

        let state = RuntimeState::load_from_support_dir(support_dir.clone());
        let store = initial_project_view_store(&state);
        let project_b = store
            .get("project-b")
            .expect("project b should be preloaded from state.json");

        assert_eq!(
            project_b.worktrees.selected_worktree_id.as_deref(),
            Some("task-b")
        );
        assert_eq!(project_b.worktrees.worktrees.len(), 1);
        assert_eq!(project_b.worktrees.worktrees[0].name, "Task B");
        assert_eq!(project_b.worktrees.tasks.len(), 1);
        assert_eq!(project_b.worktrees.tasks[0].title, "Persisted task B");

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn initial_worktree_view_store_isolates_terminal_layouts_by_project_and_worktree() {
        let support_dir = temp_support_dir("terminal-view-store");
        fs::create_dir_all(&support_dir).unwrap();
        fs::write(
            support_dir.join("state.json"),
            r#"{
                "projects": [
                    {"id": "project-a", "name": "A", "path": "/tmp/a"},
                    {"id": "project-b", "name": "B", "path": "/tmp/b"}
                ],
                "selectedProjectId": "project-a",
                "worktrees": [
                    {"id": "task-shared", "projectId": "project-a", "name": "Task A", "branch": "feature/a", "path": "/tmp/a-task", "status": "todo", "isDefault": false},
                    {"id": "task-shared", "projectId": "project-b", "name": "Task B", "branch": "feature/b", "path": "/tmp/b-task", "status": "todo", "isDefault": false}
                ],
                "selectedWorktreeIdByProject": {
                    "project-a": "task-shared",
                    "project-b": "task-shared"
                }
            }"#,
        )
        .unwrap();

        let terminal_layout_service = TerminalLayoutService::new(support_dir.clone());
        terminal_layout_service
            .save_from_gpui(
                &terminal_layout_storage_key("project-a", "task-shared"),
                Vec::new(),
                "term-project-a".to_string(),
                vec![TerminalPaneSummary {
                    title: "Project A terminal".to_string(),
                    terminal_id: "term-project-a".to_string(),
                }],
            )
            .unwrap();
        terminal_layout_service
            .save_from_gpui(
                &terminal_layout_storage_key("project-b", "task-shared"),
                Vec::new(),
                "term-project-b".to_string(),
                vec![TerminalPaneSummary {
                    title: "Project B terminal".to_string(),
                    terminal_id: "term-project-b".to_string(),
                }],
            )
            .unwrap();

        let state = RuntimeState::load_from_support_dir(support_dir.clone());
        let project_store = initial_project_view_store(&state);
        let store = initial_worktree_view_store(&state, &project_store);
        let project_a = store
            .get(&crate::app::app_state::WorktreeViewStoreKey {
                project_id: "project-a".to_string(),
                worktree_id: "task-shared".to_string(),
            })
            .expect("project a terminal layout should load");
        let project_b = store
            .get(&crate::app::app_state::WorktreeViewStoreKey {
                project_id: "project-b".to_string(),
                worktree_id: "task-shared".to_string(),
            })
            .expect("project b terminal layout should load");

        assert_eq!(
            project_a.terminal.terminal_layout.top_panes[0].title,
            "Project A terminal"
        );
        assert_eq!(
            project_b.terminal.terminal_layout.top_panes[0].title,
            "Project B terminal"
        );

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn initial_worktree_view_store_keeps_ai_history_on_current_worktree_only() {
        let support_dir = temp_support_dir("worktree-ai-history-store");
        fs::create_dir_all(&support_dir).unwrap();
        fs::write(
            support_dir.join("state.json"),
            r#"{
                "projects": [
                    {"id": "project-a", "name": "A", "path": "/tmp/a"}
                ],
                "selectedProjectId": "project-a",
                "worktrees": [
                    {"id": "task-a", "projectId": "project-a", "name": "Task A", "branch": "feature/a", "path": "/tmp/a-task", "status": "todo", "isDefault": false},
                    {"id": "task-b", "projectId": "project-a", "name": "Task B", "branch": "feature/b", "path": "/tmp/b-task", "status": "todo", "isDefault": false}
                ],
                "selectedWorktreeIdByProject": {
                    "project-a": "task-a"
                }
            }"#,
        )
        .unwrap();

        let mut state = RuntimeState::load_from_support_dir(support_dir.clone());
        state.ai_history = AIHistorySummary {
            sessions: vec![AISessionSummary {
                id: "session-a".to_string(),
                session_key: "session-a".to_string(),
                external_session_id: None,
                title: "Task A session".to_string(),
                source: "codex".to_string(),
                last_model: None,
                last_seen_at: 1.0,
                total_tokens: 10,
                cached_input_tokens: 0,
                request_count: 1,
            }],
            session_count: 1,
            indexed: true,
            indexed_at: Some(1.0),
            ..AIHistorySummary::default()
        };

        let project_store = initial_project_view_store(&state);
        let store = initial_worktree_view_store(&state, &project_store);
        let task_a = store
            .get(&crate::app::app_state::WorktreeViewStoreKey {
                project_id: "project-a".to_string(),
                worktree_id: "task-a".to_string(),
            })
            .expect("selected worktree should keep current AI history");
        let task_b = store
            .get(&crate::app::app_state::WorktreeViewStoreKey {
                project_id: "project-a".to_string(),
                worktree_id: "task-b".to_string(),
            })
            .expect("other worktree should exist");

        assert_eq!(task_a.ai_history.sessions[0].id, "session-a");
        assert!(task_b.ai_history.sessions.is_empty());

        fs::remove_dir_all(support_dir).ok();
    }

    #[test]
    fn empty_worktree_view_state_does_not_copy_current_ui_state() {
        let state = empty_worktree_view_state();

        assert!(state.ai_history.sessions.is_empty());
        assert!(state.files.files.is_empty());
        assert!(state.files.file_editor_tabs.is_empty());
        assert!(state.git.git.changed_files.is_empty());
        assert_eq!(state.git.git.staged, 0);
        assert_eq!(state.git.git.unstaged, 0);
        assert_eq!(state.git.git.untracked, 0);
        assert!(state.git.git_review.files.is_empty());
        assert!(state.terminal.terminal_layout.top_panes.is_empty());
        assert!(state.terminal.terminal_layout.tabs.is_empty());
        assert!(state.terminal.terminal_runtime.sessions.is_empty());
    }

    #[test]
    fn terminal_restore_plan_uses_terminal_ids_for_top_panes_and_bottom_tabs() {
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
            bottom_ratio: 0.32,
            error: None,
        };

        let runtime = TerminalRuntimeSummary {
            sessions: vec![TerminalRuntimeSessionSummary {
                terminal_id: "term-a".to_string(),
                pane_index: 0,
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
                source: "gpui".to_string(),
            }],
            ..Default::default()
        };
        let plan = terminal_restore_plan(&layout, &runtime);

        assert_eq!(plan.tabs.len(), 3);
        assert_eq!(plan.tabs[0].placement, TerminalTabPlacement::Top);
        assert_eq!(
            plan.tabs[0]
                .panes
                .iter()
                .map(|pane| pane.title.as_str())
                .collect::<Vec<_>>(),
            vec!["分屏 1", "长任务"]
        );
        assert_eq!(plan.tabs[0].panes[0].terminal_id.as_deref(), Some("term-a"));
        assert_eq!(
            plan.tabs[0].panes[0].restored_output_tail,
            "restored top output"
        );
        assert_eq!(plan.tabs[1].placement, TerminalTabPlacement::Bottom);
        assert_eq!(plan.tabs[1].terminal_id.as_deref(), Some("term-c"));
        assert_eq!(plan.tabs[2].terminal_id.as_deref(), Some("term-d"));
        assert_eq!(plan.active_index, 2);
    }

    #[test]
    fn terminal_restore_state_keeps_valid_terminal_ids() {
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
            bottom_ratio: 0.32,
            error: None,
        };
        let runtime = TerminalRuntimeSummary {
            active_terminal_id: "gpui-term-worktree-1-top-1".to_string(),
            sessions: vec![TerminalRuntimeSessionSummary {
                terminal_id: "gpui-term-worktree-1-top-1".to_string(),
                pane_index: 0,
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
                source: "gpui".to_string(),
            }],
            ..Default::default()
        };

        let (layout, runtime) = normalize_terminal_restore_state(
            Some("worktree-1"),
            layout,
            runtime,
            "simplifiedChinese",
        );
        let plan = terminal_restore_plan(&layout, &runtime);

        assert_eq!(layout.active_terminal_id, "gpui-term-worktree-1-bottom-1");
        assert_eq!(
            layout.top_panes[0].terminal_id,
            "gpui-term-worktree-1-top-1"
        );
        assert_eq!(layout.tabs[0].terminal_id, "gpui-term-worktree-1-bottom-1");
        assert_eq!(runtime.sessions.len(), 1);
        assert_eq!(plan.active_index, 1);
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
            bottom_ratio: 0.32,
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
        assert_eq!(layout.active_terminal_id, layout.top_panes[0].terminal_id);
        assert!(
            layout
                .active_terminal_id
                .starts_with("gpui-term-worktree-1-")
        );
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
                placement: TerminalTabPlacement::Top,
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
            bottom_ratio: 0.32,
            error: None,
        };

        let layout = structural_terminal_layout(layout);

        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].title, "kept");
        assert!(layout.tabs.is_empty());
        assert_eq!(layout.active_terminal_id, "term-2");
    }

    #[test]
    fn ai_session_restore_command_matches_tauri_history_restore() {
        let mut session = AISessionSummary {
            id: "local-id".to_string(),
            session_key: "session key".to_string(),
            external_session_id: Some("external-1".to_string()),
            title: "Task".to_string(),
            source: "codex".to_string(),
            last_model: None,
            last_seen_at: 0.0,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
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
            "opencode run --session 'session key'"
        );

        session.source = "antigravity".to_string();
        assert_eq!(
            ai_session_restore_command(&session),
            "agy resume 'session key'"
        );
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
    fn project_badge_text_uses_first_two_non_space_chars() {
        assert_eq!(
            project_badge_text_from_name(" Codux GPUI "),
            Some("CO".to_string())
        );
        assert_eq!(
            project_badge_text_from_name("项目"),
            Some("项目".to_string())
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
            "⌘⇧\\"
        } else {
            "Ctrl+Shift+\\"
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

    fn temp_support_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("codux-gpui-{label}-{nanos}"))
    }
}
