import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_runtime_core;

import '../models/remote_models.dart';
import 'remote_terminal_scope.dart';

class RemoteRuntimeState {
  const RemoteRuntimeState({
    this.projects = const [],
    this.terminals = const [],
    this.worktrees = const [],
    this.selectedProjectId,
    this.activeSessionId,
    this.selectedWorktreeId,
    this.pendingProjectSelectId,
    this.pendingProjectSelectSent = false,
    this.projectSelectAcknowledgedId,
    this.creatingTerminalProjectId,
    this.lastTerminalIdByProject = const {},
    this.baseBranchesByProject = const {},
    this.defaultBaseBranchByProject = const {},
    this.gitStatusByProject = const {},
  });

  final List<ProjectInfo> projects;
  final List<TerminalInfo> terminals;
  final List<RemoteWorktreeInfo> worktrees;
  final String? selectedProjectId;
  final String? activeSessionId;
  final String? selectedWorktreeId;
  final String? pendingProjectSelectId;
  final bool pendingProjectSelectSent;
  final String? projectSelectAcknowledgedId;
  final String? creatingTerminalProjectId;
  final Map<String, String> lastTerminalIdByProject;
  final Map<String, List<String>> baseBranchesByProject;
  final Map<String, String> defaultBaseBranchByProject;
  final Map<String, RemoteGitStatusInfo> gitStatusByProject;
}

class RemoteRuntimePlan {
  const RemoteRuntimePlan({
    this.stateChanged = false,
    this.clearTerminal = false,
    this.resetTerminalInput = false,
    this.resetTerminalBuffer = false,
    this.requestTerminalList = false,
    this.requestProjectSelectId,
    this.bindSessionId,
    this.bindFullBuffer = false,
    this.flushTerminalInput = false,
    this.removedSessionId,
  });

  final bool stateChanged;
  final bool clearTerminal;
  final bool resetTerminalInput;
  final bool resetTerminalBuffer;
  final bool requestTerminalList;
  final String? requestProjectSelectId;
  final String? bindSessionId;
  final bool bindFullBuffer;
  final bool flushTerminalInput;
  final String? removedSessionId;

  bool get hasEffect =>
      stateChanged ||
      clearTerminal ||
      resetTerminalInput ||
      resetTerminalBuffer ||
      requestTerminalList ||
      requestProjectSelectId != null ||
      bindSessionId != null ||
      bindFullBuffer ||
      flushTerminalInput ||
      removedSessionId != null;

  bool get hasRuntimeAction =>
      clearTerminal ||
      resetTerminalInput ||
      resetTerminalBuffer ||
      requestTerminalList ||
      requestProjectSelectId != null ||
      bindSessionId != null ||
      bindFullBuffer ||
      flushTerminalInput ||
      removedSessionId != null;
}

class RemoteRuntimeStore {
  RemoteRuntimeStore();

  final codux_runtime_core.RemoteRuntimeCore _core =
      codux_runtime_core.RemoteRuntimeCore();

  RemoteRuntimeState get state => _stateFromCore();
  List<ProjectInfo> get projects => state.projects;
  List<TerminalInfo> get terminals => state.terminals;
  List<RemoteWorktreeInfo> get worktrees => state.worktrees;
  String? get selectedProjectId => state.selectedProjectId;
  String? get activeSessionId => state.activeSessionId;
  String? get selectedWorktreeId => state.selectedWorktreeId;
  String? get creatingTerminalProjectId => state.creatingTerminalProjectId;
  Map<String, String> get lastTerminalIdByProject =>
      state.lastTerminalIdByProject;
  List<String> baseBranchesForProject(String projectId) =>
      state.baseBranchesByProject[projectId] ?? const [];
  String? defaultBaseBranchForProject(String projectId) =>
      state.defaultBaseBranchByProject[projectId];
  bool hasWorktreesForProject(String projectId) {
    return state.worktrees.any((worktree) => worktree.projectId == projectId);
  }

  RemoteGitStatusInfo? gitStatusForProject(String projectId) =>
      state.gitStatusByProject[projectId];

  RemoteGitStatusInfo? get selectedGitStatus {
    final projectId = selectedProjectId;
    return projectId == null ? null : state.gitStatusByProject[projectId];
  }

  RemoteTerminalScope? terminalScopeForProject(String projectId) {
    final scope = _core.terminalScopeForProject(projectId);
    return scope == null ? null : RemoteTerminalScope.fromJson(scope);
  }

  RemoteTerminalScope? terminalScopeForSession(
    String sessionId, {
    TerminalInfo? terminal,
  }) {
    final scope = _core.terminalScopeForSession(
      sessionId: sessionId,
      terminal: terminal == null ? null : _terminalToJson(terminal),
    );
    return scope == null ? null : RemoteTerminalScope.fromJson(scope);
  }

  void reset({bool keepProjects = false}) {
    _core.reset(keepProjects: keepProjects);
  }

  void restoreCachedProjects(List<ProjectInfo> projects) {
    _core.restoreCachedProjects(projects.map(_projectToJson).toList());
  }

  RemoteRuntimePlan applyProjectList({
    required List<ProjectInfo> projects,
    required String? remoteSelectedProjectId,
    required String? remoteSelectedWorktreeId,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    return _planFromCore(
      _core.applyProjectList(
        projects: projects.map(_projectToJson).toList(),
        remoteSelectedProjectId: remoteSelectedProjectId,
        remoteSelectedWorktreeId: remoteSelectedWorktreeId,
        terminalVisible: terminalVisible,
        terminalListLoaded: terminalListLoaded,
      ),
    );
  }

  RemoteRuntimePlan applyTerminalList({
    required List<TerminalInfo> terminals,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    return _planFromCore(
      _core.applyTerminalList(
        terminals: terminals.map(_terminalToJson).toList(),
        terminalVisible: terminalVisible,
        terminalListLoaded: terminalListLoaded,
      ),
    );
  }

  RemoteRuntimePlan userSelectProject({
    required ProjectInfo project,
    required bool terminalVisible,
  }) {
    return _planFromCore(
      _core.userSelectProject(
        project: _projectToJson(project),
        terminalVisible: terminalVisible,
      ),
    );
  }

  RemoteRuntimePlan projectSelected({
    required String? projectId,
    required String? worktreeId,
  }) {
    return _planFromCore(
      _core.projectSelected(projectId: projectId, worktreeId: worktreeId),
    );
  }

  RemoteRuntimePlan worktreeSelected({
    required String? projectId,
    required String? worktreeId,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    return _planFromCore(
      _core.worktreeSelected(
        projectId: projectId,
        worktreeId: worktreeId,
        terminalVisible: terminalVisible,
        terminalListLoaded: terminalListLoaded,
      ),
    );
  }

  RemoteRuntimePlan applyWorktreeState({
    required List<RemoteWorktreeInfo> worktrees,
    required String? projectId,
    required String? selectedWorktreeId,
    required List<String> baseBranches,
    required String? defaultBaseBranch,
    required bool allowRuntimeSelection,
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    final state = <String, dynamic>{
      'worktrees': worktrees.map((item) => item.toJson()).toList(),
      'baseBranches': baseBranches,
    };
    if (projectId != null) {
      state['projectId'] = projectId;
    }
    if (selectedWorktreeId != null) {
      state['selectedWorktreeId'] = selectedWorktreeId;
    }
    if (defaultBaseBranch != null) {
      state['defaultBaseBranch'] = defaultBaseBranch;
    }
    return _planFromCore(
      _core.applyWorktreeState(
        state: state,
        allowRuntimeSelection: allowRuntimeSelection,
        terminalVisible: terminalVisible,
        terminalListLoaded: terminalListLoaded,
      ),
    );
  }

  RemoteRuntimePlan ensureTerminalForSelectedProject({
    required bool terminalVisible,
    required bool terminalListLoaded,
  }) {
    return _planFromCore(
      _core.ensureTerminalForSelectedProject(
        terminalVisible: terminalVisible,
        terminalListLoaded: terminalListLoaded,
      ),
    );
  }

  RemoteRuntimePlan selectTerminal(TerminalInfo terminal) {
    return _planFromCore(_core.selectTerminal(_terminalToJson(terminal)));
  }

  RemoteRuntimePlan removeTerminal(String terminalId) {
    return _planFromCore(_core.removeTerminal(terminalId));
  }

  void setTerminalCreatingProject(String? projectId) {
    _core.setTerminalCreatingProject(projectId);
  }

  RemoteRuntimePlan terminalCreated(TerminalInfo terminal) {
    return _planFromCore(_core.terminalCreated(_terminalToJson(terminal)));
  }

  RemoteRuntimePlan applyGitStatus(RemoteGitStatusInfo status) {
    if (status.projectId.isEmpty) return const RemoteRuntimePlan();
    return _planFromCore(_core.applyGitStatus(status.toJson()));
  }

  ProjectInfo? selectedProject() {
    final id = selectedProjectId;
    if (id == null) return null;
    for (final project in projects) {
      if (project.id == id) return project;
    }
    return null;
  }

  TerminalInfo? activeTerminal() {
    final id = activeSessionId;
    if (id == null) return null;
    for (final terminal in terminals) {
      if (terminal.id == id) return terminal;
    }
    return null;
  }

  void markProjectSelectSent(String projectId) {
    _core.markProjectSelectSent(projectId);
  }

  void clearPendingProjectSelectSent(String projectId) {
    _core.clearProjectSelectSent(projectId);
  }

  String? pendingProjectSelect({bool includeSent = false}) {
    return _core.pendingProjectSelect(includeSent: includeSent);
  }

  List<TerminalInfo> currentProjectTerminals() {
    return _core.currentProjectTerminals().map(TerminalInfo.fromJson).toList();
  }

  static bool isAccessibleTerminal(TerminalInfo terminal) =>
      terminal.id.isNotEmpty && terminal.projectId.isNotEmpty;

  RemoteRuntimeState _stateFromCore() {
    final snapshot = _core.snapshot();
    return RemoteRuntimeState(
      projects: snapshot.projects.map(ProjectInfo.fromJson).toList(),
      terminals: snapshot.terminals.map(TerminalInfo.fromJson).toList(),
      worktrees: snapshot.worktrees.map(RemoteWorktreeInfo.fromJson).toList(),
      selectedProjectId: snapshot.selectedProjectId,
      activeSessionId: snapshot.activeSessionId,
      selectedWorktreeId: snapshot.selectedWorktreeId,
      pendingProjectSelectId: snapshot.pendingProjectSelectId,
      pendingProjectSelectSent: snapshot.pendingProjectSelectSent,
      projectSelectAcknowledgedId: snapshot.projectSelectAcknowledgedId,
      creatingTerminalProjectId: snapshot.creatingTerminalProjectId,
      lastTerminalIdByProject: snapshot.lastTerminalIdByProject,
      baseBranchesByProject: snapshot.baseBranchesByProject,
      defaultBaseBranchByProject: snapshot.defaultBaseBranchByProject,
      gitStatusByProject: {
        for (final entry in snapshot.gitStatusByProject.entries)
          entry.key: RemoteGitStatusInfo.fromJson(entry.value),
      },
    );
  }
}

RemoteRuntimePlan _planFromCore(codux_runtime_core.RemoteRuntimeCorePlan plan) {
  return RemoteRuntimePlan(
    stateChanged: plan.stateChanged,
    clearTerminal: plan.clearTerminal,
    resetTerminalInput: plan.resetTerminalInput,
    resetTerminalBuffer: plan.resetTerminalBuffer,
    requestTerminalList: plan.requestTerminalList,
    requestProjectSelectId: plan.requestProjectSelectId,
    bindSessionId: plan.bindSessionId,
    bindFullBuffer: plan.bindFullBuffer,
    flushTerminalInput: plan.flushTerminalInput,
    removedSessionId: plan.removedSessionId,
  );
}

Map<String, dynamic> _projectToJson(ProjectInfo project) => project.toJson();

Map<String, dynamic> _terminalToJson(TerminalInfo terminal) => {
  'id': terminal.id,
  'title': terminal.title,
  'projectId': terminal.projectId,
  'layoutKind': terminal.layoutKind,
  if (terminal.worktreeId != null) 'worktreeId': terminal.worktreeId,
  if (terminal.layoutOrder != null) 'layoutOrder': terminal.layoutOrder,
  if (terminal.cols != null) 'cols': terminal.cols,
  if (terminal.rows != null) 'rows': terminal.rows,
  if (terminal.status != null) 'status': terminal.status,
  if (terminal.createdAt != null) 'createdAt': terminal.createdAt,
  if (terminal.bufferCharacters != null)
    'bufferCharacters': terminal.bufferCharacters,
};
