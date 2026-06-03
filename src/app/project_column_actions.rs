use super::ai_runtime_status::AIActivityState;
use super::*;

impl CoduxApp {
    pub(super) fn selected_project_id(&self) -> Option<String> {
        self.state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone())
    }

    pub(super) fn ensure_project_list_store(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ProjectListStore> {
        if let Some(store) = &self.project_list_store {
            return store.clone();
        }

        let activity = self.project_activity_snapshot();
        let store = cx.new(|_| {
            let mut store =
                ProjectListStore::new(self.state.projects.clone(), self.selected_project_id());
            store.activity = activity;
            store
        });
        self.project_list_store = Some(store.clone());
        store
    }

    pub(super) fn sync_project_list_store(&mut self, cx: &mut Context<Self>) {
        let store = self.ensure_project_list_store(cx);
        let projects = self.state.projects.clone();
        let selected_project_id = self.selected_project_id();
        let activity = self.project_activity_snapshot();
        store.update(cx, |store, cx| {
            store.set_snapshot(projects, selected_project_id, cx);
            store.set_activity(activity, cx);
        });
    }

    pub(super) fn sync_project_activity_store(&mut self, cx: &mut Context<Self>) {
        let store = self.ensure_project_list_store(cx);
        let activity = self.project_activity_snapshot();
        store.update(cx, |store, cx| store.set_activity(activity, cx));
    }

    fn project_activity_snapshot(&self) -> HashMap<String, AIActivityState> {
        let worktree_activity = self
            .state
            .worktrees
            .worktrees
            .iter()
            .map(|worktree| (worktree.id.clone(), self.ai_activity_for_worktree(worktree)))
            .collect::<HashMap<_, _>>();
        self.state
            .projects
            .iter()
            .map(|project| {
                (
                    project.id.clone(),
                    super::ai_runtime_status::aggregate_project_activity(
                        self.ai_activity_for_project(project),
                        &project.id,
                        &self.state.worktrees.worktrees,
                        &worktree_activity,
                    ),
                )
            })
            .collect()
    }

    pub(in crate::app) fn visible_pet_sprite_frame(&self, frame_count: usize) -> usize {
        if self.state.settings.pet_static_mode {
            0
        } else {
            self.pet_sprite_frame % frame_count.max(1)
        }
    }

    pub(super) fn project_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<ProjectColumnView> {
        let app_entity = cx.entity();
        let project_store = self.ensure_project_list_store(cx);
        let collapsed = self.project_column_collapsed;
        let language = self.state.settings.language.clone();
        let has_project = self.state.selected_project.is_some();
        let has_projects = !self.state.projects.is_empty();
        let has_worktree = self.state.worktrees.selected_worktree_id.is_some();
        let scroll_handle = self.project_scroll_handle.clone();

        if let Some(view) = &self.project_column_view {
            view.update(cx, |view, cx| {
                let changed = view.collapsed != collapsed
                    || view.language != language
                    || view.has_project != has_project
                    || view.has_projects != has_projects
                    || view.has_worktree != has_worktree;

                if !changed {
                    return;
                }

                view.collapsed = collapsed;
                view.language = language;
                view.has_project = has_project;
                view.has_projects = has_projects;
                view.has_worktree = has_worktree;
                view.scroll_handle = scroll_handle;
                cx.notify();
            });
            return view.clone();
        }
        let view = cx.new(|_| ProjectColumnView {
            app_entity: app_entity.clone(),
            project_store,
            collapsed,
            language,
            has_project,
            has_projects,
            has_worktree,
            scroll_handle,
            _observe_project_store: None,
        });
        view.update(cx, |view, cx| {
            view._observe_project_store =
                Some(cx.observe(&view.project_store, |_, _, cx| cx.notify()));
        });
        self.project_column_view = Some(view.clone());
        view
    }
}
