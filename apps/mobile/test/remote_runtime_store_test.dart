import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_runtime_store.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('git status round-trips through the runtime core by project', () {
    final store = RemoteRuntimeStore();
    const status = RemoteGitStatusInfo(
      projectId: 'project-1',
      projectPath: '/tmp/project-1',
      branch: 'main',
      ahead: 1,
      behind: 2,
      changes: 3,
      isRepository: true,
      changedFiles: [
        RemoteGitFileStatus(
          path: 'lib/main.dart',
          indexStatus: 'M',
          worktreeStatus: ' ',
        ),
      ],
    );

    final plan = store.applyGitStatus(status);
    expect(plan.stateChanged, isTrue);

    final restored = store.gitStatusForProject('project-1');
    expect(restored, isNotNull);
    expect(restored!.branch, 'main');
    expect(restored.ahead, 1);
    expect(restored.behind, 2);
    expect(restored.changes, 3);
    expect(restored.isRepository, isTrue);
    expect(restored.changedFiles.single.path, 'lib/main.dart');
    expect(store.gitStatusForProject('missing'), isNull);
  });

  test('project list uses host selected project before local cache', () {
    final store = RemoteRuntimeStore();
    store.restoreCachedProjects([
      const ProjectInfo(id: 'project-1', name: 'Project 1'),
      const ProjectInfo(id: 'project-2', name: 'Project 2'),
    ]);

    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    expect(store.selectedProjectId, 'project-2');
  });

  test('visible terminal binds existing session for selected project', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );

    final plan = store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(store.activeSessionId, 'session-2');
    expect(plan.bindSessionId, 'session-2');
    expect(plan.requestProjectSelectId, isNull);
  });

  test('worktree selection binds controller active terminal memory', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'session-1',
          title: 'One',
          projectId: 'project-2',
          worktreeId: 'project-2',
        ),
        TerminalInfo(
          id: 'session-2',
          title: 'Two',
          projectId: 'project-2',
          worktreeId: 'worktree-2',
          layoutOrder: 0,
        ),
        TerminalInfo(
          id: 'session-3',
          title: 'Three',
          projectId: 'project-2',
          worktreeId: 'worktree-2',
          layoutOrder: 1,
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final plan = store.worktreeSelected(
      projectId: 'project-2',
      worktreeId: 'worktree-2',
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(plan.bindSessionId, 'session-2');
    expect(plan.requestProjectSelectId, isNull);
    expect(store.activeSessionId, 'session-2');
    expect(store.selectedWorktreeId, 'worktree-2');
    expect(store.currentProjectTerminals().map((terminal) => terminal.id), [
      'session-2',
      'session-3',
    ]);

    store.selectTerminal(
      const TerminalInfo(
        id: 'session-3',
        title: 'Three',
        projectId: 'project-2',
        worktreeId: 'worktree-2',
        layoutOrder: 1,
      ),
    );
    store.worktreeSelected(
      projectId: 'project-2',
      worktreeId: 'project-2',
      terminalVisible: true,
      terminalListLoaded: true,
    );
    final back = store.worktreeSelected(
      projectId: 'project-2',
      worktreeId: 'worktree-2',
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(back.bindSessionId, 'session-3');
  });

  test('project list worktree scope binds controller task terminal', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: 'worktree-2',
      terminalVisible: true,
      terminalListLoaded: false,
    );

    final plan = store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'default-session',
          title: 'Default',
          projectId: 'project-2',
          worktreeId: 'project-2',
          layoutOrder: 0,
        ),
        TerminalInfo(
          id: 'worktree-session',
          title: 'Task',
          projectId: 'project-2',
          worktreeId: 'worktree-2',
          layoutOrder: 0,
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(plan.bindSessionId, 'worktree-session');
    expect(store.selectedWorktreeId, 'worktree-2');
    expect(store.activeSessionId, 'worktree-session');
  });

  test('project switch restores each project worktree and terminal scope', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyWorktreeState(
      projectId: 'project-1',
      selectedWorktreeId: 'project-1',
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'project-1',
          projectId: 'project-1',
          name: 'main',
          branch: 'main',
          path: '/tmp/project-1',
          status: 'clean',
          isDefault: true,
          exists: true,
        ),
        RemoteWorktreeInfo(
          id: 'worktree-1',
          projectId: 'project-1',
          name: 'Task',
          branch: 'task',
          path: '/tmp/project-1-task',
          status: 'clean',
          isDefault: false,
          exists: true,
        ),
      ],
      baseBranches: const ['main'],
      defaultBaseBranch: 'main',
      allowRuntimeSelection: false,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'project-1-main',
          title: 'Main',
          projectId: 'project-1',
          worktreeId: 'project-1',
          layoutOrder: 0,
        ),
        TerminalInfo(
          id: 'project-1-task',
          title: 'Task',
          projectId: 'project-1',
          worktreeId: 'worktree-1',
          layoutOrder: 0,
        ),
        TerminalInfo(
          id: 'project-2-main',
          title: 'Main',
          projectId: 'project-2',
          worktreeId: 'project-2',
          layoutOrder: 0,
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final selectedWorktree = store.worktreeSelected(
      projectId: 'project-1',
      worktreeId: 'worktree-1',
      terminalVisible: true,
      terminalListLoaded: true,
    );
    final selectedProject2 = store.userSelectProject(
      project: _projects[1],
      terminalVisible: true,
    );
    final backToProject1 = store.userSelectProject(
      project: _projects[0],
      terminalVisible: true,
    );

    expect(selectedWorktree.bindSessionId, 'project-1-task');
    expect(selectedProject2.bindSessionId, 'project-2-main');
    expect(backToProject1.bindSessionId, 'project-1-task');
    expect(store.selectedProjectId, 'project-1');
    expect(store.selectedWorktreeId, 'worktree-1');
    expect(store.activeSessionId, 'project-1-task');
  });

  test('worktree selection does not repeat terminal list for stale list', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'default-session',
          title: 'Default',
          projectId: 'project-2',
          worktreeId: 'project-2',
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final selected = store.worktreeSelected(
      projectId: 'project-2',
      worktreeId: 'worktree-2',
      terminalVisible: true,
      terminalListLoaded: true,
    );
    final staleList = store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'default-session',
          title: 'Default',
          projectId: 'project-2',
          worktreeId: 'project-2',
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(selected.bindSessionId, isNull);
    expect(selected.requestProjectSelectId, isNull);
    expect(selected.requestTerminalList, isTrue);
    expect(staleList.bindSessionId, isNull);
    expect(staleList.requestProjectSelectId, isNull);
    expect(staleList.requestTerminalList, isFalse);
    expect(store.activeSessionId, isNull);
    expect(store.selectedWorktreeId, 'worktree-2');
  });

  test(
    'missing terminal requests one project select until terminal appears',
    () {
      final store = RemoteRuntimeStore();
      store.applyProjectList(
        projects: _projects,
        remoteSelectedProjectId: 'project-2',
        remoteSelectedWorktreeId: null,
        terminalVisible: true,
        terminalListLoaded: false,
      );

      final first = store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );
      final second = store.ensureTerminalForSelectedProject(
        terminalVisible: true,
        terminalListLoaded: true,
      );

      expect(first.requestProjectSelectId, 'project-2');
      expect(second.requestProjectSelectId, 'project-2');
      store.markProjectSelectSent('project-2');
      final third = store.ensureTerminalForSelectedProject(
        terminalVisible: true,
        terminalListLoaded: true,
      );
      expect(third.requestProjectSelectId, isNull);

      final fourth = store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
          TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );

      expect(store.activeSessionId, 'session-2');
      expect(fourth.bindSessionId, 'session-2');
    },
  );

  test('user project selection immediately binds known project terminal', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final select = store.userSelectProject(
      project: _projects[1],
      terminalVisible: true,
    );
    final beforeHost = store.ensureTerminalForSelectedProject(
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(select.requestProjectSelectId, 'project-2');
    expect(select.clearTerminal, isTrue);
    expect(select.bindSessionId, 'session-2');
    expect(select.bindFullBuffer, isTrue);
    expect(beforeHost.requestProjectSelectId, isNull);
    expect(beforeHost.bindSessionId, isNull);
    expect(store.activeSessionId, 'session-2');
  });

  test(
    'user project selection requests host select when local terminal is unknown',
    () {
      final store = RemoteRuntimeStore();
      store.applyProjectList(
        projects: _projects,
        remoteSelectedProjectId: 'project-1',
        remoteSelectedWorktreeId: null,
        terminalVisible: true,
        terminalListLoaded: false,
      );
      store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );

      final select = store.userSelectProject(
        project: _projects[1],
        terminalVisible: true,
      );

      expect(select.requestProjectSelectId, 'project-2');
      expect(select.clearTerminal, isTrue);
      expect(select.requestTerminalList, isTrue);
      expect(select.bindSessionId, isNull);
      expect(store.activeSessionId, isNull);

      final afterHost = store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
          TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );

      expect(afterHost.bindSessionId, 'session-2');
      expect(store.activeSessionId, 'session-2');
    },
  );

  test(
    'pending project with empty terminal list does not repeat project select',
    () {
      final store = RemoteRuntimeStore();
      store.applyProjectList(
        projects: _projects,
        remoteSelectedProjectId: 'project-1',
        remoteSelectedWorktreeId: null,
        terminalVisible: true,
        terminalListLoaded: false,
      );

      final select = store.userSelectProject(
        project: _projects[1],
        terminalVisible: true,
      );
      final emptyList = store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );
      final ensure = store.ensureTerminalForSelectedProject(
        terminalVisible: true,
        terminalListLoaded: true,
      );

      expect(select.requestProjectSelectId, 'project-2');
      expect(select.requestTerminalList, isTrue);
      expect(emptyList.requestProjectSelectId, 'project-2');
      expect(emptyList.requestTerminalList, isFalse);
      expect(ensure.requestProjectSelectId, 'project-2');
      expect(ensure.requestTerminalList, isFalse);
      store.markProjectSelectSent('project-2');
      final afterSent = store.ensureTerminalForSelectedProject(
        terminalVisible: true,
        terminalListLoaded: true,
      );
      expect(afterSent.requestProjectSelectId, isNull);
      expect(afterSent.requestTerminalList, isFalse);
      expect(store.activeSessionId, isNull);
    },
  );

  test('pending user project selection beats stale host selected project', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );

    store.userSelectProject(project: _projects[1], terminalVisible: true);
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );

    expect(store.selectedProjectId, 'project-2');
  });

  test('failed pending project select is planned again until sent', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );

    final select = store.userSelectProject(
      project: _projects[1],
      terminalVisible: true,
    );
    expect(select.requestProjectSelectId, 'project-2');

    final retry = store.ensureTerminalForSelectedProject(
      terminalVisible: true,
      terminalListLoaded: true,
    );
    expect(retry.requestProjectSelectId, 'project-2');

    store.markProjectSelectSent('project-2');
    final afterSent = store.ensureTerminalForSelectedProject(
      terminalVisible: true,
      terminalListLoaded: true,
    );
    expect(afterSent.requestProjectSelectId, isNull);
  });

  test('project selected confirmation clears pending project select', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.userSelectProject(project: _projects[1], terminalVisible: true);
    store.markProjectSelectSent('project-2');

    expect(store.pendingProjectSelect(), isNull);
    expect(store.pendingProjectSelect(includeSent: true), 'project-2');

    final confirmed = store.projectSelected(
      projectId: 'project-2',
      worktreeId: null,
    );
    final retry = store.ensureTerminalForSelectedProject(
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(confirmed.requestTerminalList, isTrue);
    expect(store.pendingProjectSelect(), isNull);
    expect(store.pendingProjectSelect(includeSent: true), isNull);
    expect(retry.requestProjectSelectId, isNull);
  });

  test('project selected confirmation waits for terminal list', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.userSelectProject(project: _projects[1], terminalVisible: true);

    final confirmed = store.projectSelected(
      projectId: 'project-2',
      worktreeId: null,
    );

    expect(store.selectedProjectId, 'project-2');
    expect(confirmed.requestTerminalList, isTrue);
    expect(confirmed.requestProjectSelectId, isNull);

    final beforeList = store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(beforeList.requestProjectSelectId, isNull);

    final terminalList = store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(terminalList.bindSessionId, 'session-2');
    expect(store.activeSessionId, 'session-2');
  });

  test('project selected confirmation drops old active terminal session', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final confirmed = store.projectSelected(
      projectId: 'project-2',
      worktreeId: null,
    );

    expect(confirmed.requestTerminalList, isTrue);
    expect(confirmed.resetTerminalBuffer, isTrue);
    expect(store.selectedProjectId, 'project-2');
    expect(store.activeSessionId, isNull);

    final terminalList = store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(terminalList.bindSessionId, 'session-2');
    expect(store.activeSessionId, 'session-2');
  });

  test('terminal list selects active session before viewport is ready', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    final plan = store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: false,
      terminalListLoaded: true,
    );

    expect(plan.bindSessionId, 'session-2');
    expect(plan.bindFullBuffer, isFalse);
    expect(store.activeSessionId, 'session-2');
  });

  test('background reset keeps project but drops terminal session state', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    store.reset(keepProjects: true);

    expect(
      store.projects.map((project) => project.toJson()).toList(),
      _projects.map((project) => project.toJson()).toList(),
    );
    expect(store.selectedProjectId, 'project-2');
    expect(store.activeSessionId, isNull);
    expect(store.terminals, isEmpty);
  });

  test('stores git status by project in runtime state', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-2',
      remoteSelectedWorktreeId: null,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    final plan = store.applyGitStatus(
      const RemoteGitStatusInfo(
        projectId: 'project-2',
        projectPath: '/tmp/project-2',
        branch: 'main',
        ahead: 2,
        behind: 1,
        staged: 1,
        unstaged: 2,
        untracked: 3,
        changes: 6,
        isRepository: true,
      ),
    );

    expect(plan.stateChanged, isTrue);
    expect(store.selectedGitStatus?.branch, 'main');
    expect(store.selectedGitStatus?.changes, 6);
  });

  test('stores worktree lists by project in runtime core', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    store.applyWorktreeState(
      projectId: 'project-1',
      selectedWorktreeId: 'project-1',
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'project-1',
          projectId: 'project-1',
          name: 'Project 1',
          branch: 'main',
          path: '/tmp/project-1',
          status: 'clean',
          isDefault: true,
          exists: true,
        ),
      ],
      baseBranches: const ['main'],
      defaultBaseBranch: 'main',
      allowRuntimeSelection: false,
      terminalVisible: false,
      terminalListLoaded: false,
    );
    store.applyWorktreeState(
      projectId: 'project-2',
      selectedWorktreeId: 'worktree-2',
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'worktree-2',
          projectId: 'project-2',
          name: 'Task',
          branch: 'feature/task',
          path: '/tmp/project-2-task',
          status: 'clean',
          isDefault: false,
          exists: true,
        ),
      ],
      baseBranches: const ['develop'],
      defaultBaseBranch: 'develop',
      allowRuntimeSelection: false,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    expect(store.worktrees.map((worktree) => worktree.projectId).toSet(), {
      'project-1',
      'project-2',
    });
    expect(store.baseBranchesForProject('project-1'), ['main']);
    expect(store.defaultBaseBranchForProject('project-2'), 'develop');
  });

  test('initializes selected worktree from default runtime worktree', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    store.applyWorktreeState(
      projectId: 'project-1',
      selectedWorktreeId: null,
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'worktree-1',
          projectId: 'project-1',
          name: 'Task',
          branch: 'task',
          path: '/tmp/project-1-task',
          status: 'clean',
          isDefault: false,
          exists: true,
        ),
        RemoteWorktreeInfo(
          id: 'project-1',
          projectId: 'project-1',
          name: 'main',
          branch: 'main',
          path: '/tmp/project-1',
          status: 'clean',
          isDefault: true,
          exists: true,
        ),
      ],
      baseBranches: const ['main'],
      defaultBaseBranch: 'main',
      allowRuntimeSelection: false,
      terminalVisible: false,
      terminalListLoaded: false,
    );

    expect(store.selectedWorktreeId, 'project-1');
  });

  test('local worktree selection is not overwritten by stale list refresh', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(
          id: 'main-session',
          title: 'Main',
          projectId: 'project-1',
          worktreeId: 'project-1',
        ),
        TerminalInfo(
          id: 'worktree-session',
          title: 'Task',
          projectId: 'project-1',
          worktreeId: 'worktree-1',
        ),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );
    store.applyWorktreeState(
      projectId: 'project-1',
      selectedWorktreeId: 'project-1',
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'project-1',
          projectId: 'project-1',
          name: 'main',
          branch: 'main',
          path: '/tmp/project-1',
          status: 'clean',
          isDefault: true,
          exists: true,
        ),
        RemoteWorktreeInfo(
          id: 'worktree-1',
          projectId: 'project-1',
          name: 'Task',
          branch: 'task',
          path: '/tmp/project-1-task',
          status: 'clean',
          isDefault: false,
          exists: true,
        ),
      ],
      baseBranches: const ['main'],
      defaultBaseBranch: 'main',
      allowRuntimeSelection: false,
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final local = store.worktreeSelected(
      projectId: 'project-1',
      worktreeId: 'worktree-1',
      terminalVisible: true,
      terminalListLoaded: true,
    );
    final staleList = store.applyWorktreeState(
      projectId: 'project-1',
      selectedWorktreeId: 'project-1',
      worktrees: const [
        RemoteWorktreeInfo(
          id: 'project-1',
          projectId: 'project-1',
          name: 'main',
          branch: 'main',
          path: '/tmp/project-1',
          status: 'clean',
          isDefault: true,
          exists: true,
        ),
        RemoteWorktreeInfo(
          id: 'worktree-1',
          projectId: 'project-1',
          name: 'Task',
          branch: 'task',
          path: '/tmp/project-1-task',
          status: 'clean',
          isDefault: false,
          exists: true,
        ),
      ],
      baseBranches: const ['main'],
      defaultBaseBranch: 'main',
      allowRuntimeSelection: false,
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(local.bindSessionId, 'worktree-session');
    expect(staleList.bindSessionId, isNull);
    expect(store.selectedWorktreeId, 'worktree-1');
    expect(store.activeSessionId, 'worktree-session');
  });

  test('terminal scope follows active terminal project and path', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        TerminalInfo(id: 'session-2', title: 'Two', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final scope = store.terminalScopeForSession('session-2');

    expect(scope?.projectId, 'project-2');
    expect(scope?.projectPath, '/tmp/project-2');
  });

  test(
    'terminal scope uses explicit terminal when runtime list dropped it',
    () {
      final store = RemoteRuntimeStore();
      store.applyProjectList(
        projects: _projects,
        remoteSelectedProjectId: 'project-1',
        remoteSelectedWorktreeId: null,
        terminalVisible: true,
        terminalListLoaded: false,
      );

      final scope = store.terminalScopeForSession(
        'session-2',
        terminal: const TerminalInfo(
          id: 'session-2',
          title: 'Two',
          projectId: 'project-2',
          worktreeId: 'worktree-2',
        ),
      );

      expect(scope?.projectId, 'project-2');
      expect(scope?.worktreeId, 'worktree-2');
      expect(scope?.projectPath, '/tmp/project-2');
    },
  );

  test(
    'terminal close removes active session without changing selected project',
    () {
      final store = RemoteRuntimeStore();
      store.applyProjectList(
        projects: _projects,
        remoteSelectedProjectId: 'project-1',
        remoteSelectedWorktreeId: null,
        terminalVisible: true,
        terminalListLoaded: false,
      );
      store.applyTerminalList(
        terminals: const [
          TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
        ],
        terminalVisible: true,
        terminalListLoaded: true,
      );

      final plan = store.removeTerminal('session-1');

      expect(plan.clearTerminal, isTrue);
      expect(plan.removedSessionIds, ['session-1']);
      expect(store.selectedProjectId, 'project-1');
      expect(store.activeSessionId, isNull);
    },
  );

  test('terminal list reports every removed inactive session through FFI', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'active', title: 'Active', projectId: 'project-1'),
        TerminalInfo(id: 'inactive-a', title: 'A', projectId: 'project-2'),
        TerminalInfo(id: 'inactive-b', title: 'B', projectId: 'project-2'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    final plan = store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'active', title: 'Active', projectId: 'project-1'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );

    expect(plan.removedSessionIds, ['inactive-a', 'inactive-b']);
  });

  test('locally created terminal becomes active through FFI', () {
    final store = RemoteRuntimeStore();
    store.applyProjectList(
      projects: _projects,
      remoteSelectedProjectId: 'project-1',
      remoteSelectedWorktreeId: null,
      terminalVisible: true,
      terminalListLoaded: false,
    );
    store.applyTerminalList(
      terminals: const [
        TerminalInfo(id: 'session-1', title: 'One', projectId: 'project-1'),
      ],
      terminalVisible: true,
      terminalListLoaded: true,
    );
    store.beginTerminalCreate(terminalId: 'session-2', projectId: 'project-1');

    final plan = store.terminalCreated(
      const TerminalInfo(
        id: 'session-2',
        title: 'Two',
        projectId: 'project-1',
        layoutOrder: 1,
      ),
    );

    expect(plan.bindSessionId, 'session-2');
    expect(plan.clearTerminal, isTrue);
    expect(plan.resetTerminalInput, isTrue);
    expect(plan.resetTerminalBuffer, isTrue);
    expect(store.activeSessionId, 'session-2');
  });
}

const _projects = [
  ProjectInfo(id: 'project-1', name: 'Project 1', path: '/tmp/project-1'),
  ProjectInfo(id: 'project-2', name: 'Project 2', path: '/tmp/project-2'),
];
