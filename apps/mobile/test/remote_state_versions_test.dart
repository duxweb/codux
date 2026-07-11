import 'package:codux_flutter/services/remote_state_versions.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('project resources require the currently selected project', () {
    final versions = RemoteStateVersions();

    expect(
      versions.acceptProjectPayload('worktrees', {
        'projectId': 'project-1',
        'version': 1,
      }, currentProjectId: 'project-1'),
      isTrue,
    );
    expect(
      versions.acceptProjectPayload('worktrees', {
        'projectId': 'project-1',
        'version': 2,
      }, currentProjectId: 'project-2'),
      isFalse,
    );
    expect(
      versions.acceptProjectPayload('worktrees', {
        'version': 3,
      }, currentProjectId: 'project-1'),
      isFalse,
    );
  });

  test('project resources reject older versions for the same project', () {
    final versions = RemoteStateVersions();

    expect(
      versions.acceptProjectPayload('git.status', {
        'projectId': 'project-1',
        'version': 3,
      }, currentProjectId: 'project-1'),
      isTrue,
    );
    expect(
      versions.acceptProjectPayload('git.status', {
        'projectId': 'project-1',
        'version': 2,
      }, currentProjectId: 'project-1'),
      isFalse,
    );
  });
}
