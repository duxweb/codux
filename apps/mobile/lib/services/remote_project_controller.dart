import '../models/remote_models.dart';
import 'remote_path_utils.dart';
import 'remote_protocol.dart';

enum ProjectFormMode { add, edit }

class ProjectFormDraft {
  const ProjectFormDraft({
    required this.mode,
    required this.name,
    required this.path,
  });

  final ProjectFormMode mode;
  final String name;
  final String path;
}

class ProjectFolderSelection {
  const ProjectFolderSelection({required this.path, required this.name});

  final String path;
  final String name;
}

class ProjectSavePlan {
  const ProjectSavePlan._({
    required this.valid,
    required this.envelope,
    required this.name,
  });

  const ProjectSavePlan.invalid()
    : this._(valid: false, envelope: null, name: '');

  const ProjectSavePlan.valid({
    required RelayEnvelope envelope,
    required String name,
  }) : this._(valid: true, envelope: envelope, name: name);

  final bool valid;
  final RelayEnvelope? envelope;
  final String name;
}

class RemoteProjectController {
  const RemoteProjectController();

  ProjectFormDraft editDraft(ProjectInfo project) {
    return ProjectFormDraft(
      mode: ProjectFormMode.edit,
      name: project.name,
      path: project.path ?? '',
    );
  }

  ProjectFormDraft addDraft() {
    return const ProjectFormDraft(
      mode: ProjectFormMode.add,
      name: '',
      path: '',
    );
  }

  ProjectSavePlan savePlan({
    required ProjectFormMode mode,
    required String path,
    required String name,
    ProjectInfo? selectedProject,
  }) {
    final cleanPath = path.trim();
    if (cleanPath.isEmpty) return const ProjectSavePlan.invalid();
    final cleanName = name.trim().isEmpty
        ? remoteLastPathComponent(cleanPath)
        : name.trim();
    if (mode == ProjectFormMode.edit) {
      final project = selectedProject;
      if (project == null) return const ProjectSavePlan.invalid();
      return ProjectSavePlan.valid(
        name: cleanName,
        envelope: RelayEnvelope(
          type: RemoteMessageType.projectEdit,
          payload: {
            'projectId': project.id,
            'path': cleanPath,
            'name': cleanName,
          },
        ),
      );
    }
    return ProjectSavePlan.valid(
      name: cleanName,
      envelope: RelayEnvelope(
        type: RemoteMessageType.projectAdd,
        payload: {'path': cleanPath, 'name': cleanName},
      ),
    );
  }

  RelayEnvelope filePickerListEnvelope(String? path) {
    final cleanPath = path?.trim() ?? '';
    return RelayEnvelope(
      type: RemoteMessageType.fileList,
      payload: cleanPath.isEmpty ? <String, Object>{} : {'path': cleanPath},
    );
  }

  RelayEnvelope removeEnvelope(ProjectInfo project) {
    return RelayEnvelope(
      type: RemoteMessageType.projectRemove,
      payload: {'projectId': project.id},
    );
  }

  RelayEnvelope aiStatsEnvelope(ProjectInfo project, {String? worktreeId}) {
    final cleanWorktreeId = worktreeId?.trim();
    return RelayEnvelope(
      type: RemoteMessageType.aiStats,
      payload: {
        'projectId': project.id,
        if (cleanWorktreeId != null && cleanWorktreeId.isNotEmpty)
          'worktreeId': cleanWorktreeId,
      },
    );
  }

  /// Request the AI conversation-history list for a project (same `ai.session`
  /// channel + DTO both hosts serve). Host replies `ai.session.result`.
  RelayEnvelope aiSessionListEnvelope(ProjectInfo project) {
    return RelayEnvelope(
      type: RemoteMessageType.aiSession,
      payload: {
        'op': 'list',
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
      },
    );
  }

  /// Rename a session in the host's AI history. Host replies `ai.session.result`
  /// with op `rename`; we refresh the list afterwards.
  RelayEnvelope aiSessionRenameEnvelope(
    ProjectInfo project,
    String sessionId,
    String title,
  ) {
    return RelayEnvelope(
      type: RemoteMessageType.aiSession,
      payload: {
        'op': 'rename',
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
        'sessionId': sessionId,
        'title': title,
      },
    );
  }

  /// Remove a session from the host's AI history. Host replies with op `remove`.
  RelayEnvelope aiSessionRemoveEnvelope(ProjectInfo project, String sessionId) {
    return RelayEnvelope(
      type: RemoteMessageType.aiSession,
      payload: {
        'op': 'remove',
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
        'sessionId': sessionId,
      },
    );
  }

  /// Ask the host for the resume command that re-opens a session in its CLI tool
  /// (e.g. `claude --resume <id>`). Host replies with op `restore`; we write the
  /// returned command into the active terminal.
  RelayEnvelope aiSessionRestoreEnvelope(ProjectInfo project, String sessionId) {
    return RelayEnvelope(
      type: RemoteMessageType.aiSession,
      payload: {
        'op': 'restore',
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
        'sessionId': sessionId,
      },
    );
  }

  /// Request the host's saved SSH profiles (host-wide; the host owns them).
  RelayEnvelope sshListEnvelope() {
    return RelayEnvelope(type: RemoteMessageType.sshList, payload: const {});
  }

  /// Add or update a saved SSH profile on the host. `fields` carries the
  /// SSHProfileUpsertRequest shape (id?, name, host, port, username,
  /// credentialKind, password?, privateKeyPath?, keyPassphrase?).
  RelayEnvelope sshUpsertEnvelope(Map<String, dynamic> fields) {
    return RelayEnvelope(type: RemoteMessageType.sshUpsert, payload: fields);
  }

  RelayEnvelope sshRemoveEnvelope(String id) {
    return RelayEnvelope(
      type: RemoteMessageType.sshRemove,
      payload: {'id': id},
    );
  }

  RelayEnvelope gitStatusEnvelope(ProjectInfo project) {
    return RelayEnvelope(
      type: RemoteMessageType.gitStatus,
      payload: {
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
      },
    );
  }

  /// Generic git mutation (stage/unstage/discard/commit/push/...). The host
  /// replies with a refreshed `git.status`. Served by both desktop and agent.
  RelayEnvelope gitInvokeEnvelope(
    ProjectInfo project,
    String op, {
    Map<String, dynamic> args = const {},
  }) {
    return RelayEnvelope(
      type: RemoteMessageType.gitInvoke,
      payload: {
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
        'op': op,
        'args': args,
      },
    );
  }

  /// Generic git read query (diff/path_status/...). Host replies
  /// `git.read {op, result}`. Served by both desktop and agent for `diff`.
  RelayEnvelope gitReadEnvelope(
    ProjectInfo project,
    String op, {
    Map<String, dynamic> args = const {},
  }) {
    return RelayEnvelope(
      type: RemoteMessageType.gitRead,
      payload: {
        'projectId': project.id,
        if (project.path != null) 'projectPath': project.path,
        'op': op,
        'args': args,
      },
    );
  }

  String folderDisplayName(RemoteFileEntry entry) {
    return entry.name.isEmpty
        ? remoteLastPathComponent(entry.path)
        : entry.name;
  }

  ProjectFolderSelection selectFolder({
    required RemoteFileEntry entry,
    required String currentName,
  }) {
    final cleanName = currentName.trim();
    return ProjectFolderSelection(
      path: entry.path,
      name: cleanName.isEmpty ? folderDisplayName(entry) : cleanName,
    );
  }
}
