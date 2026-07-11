import 'package:codux_flutter/services/remote_protocol.dart';
import 'package:codux_flutter/services/remote_terminal_subscription_controller.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('first project subscription requests topology only', () {
    final controller = RemoteTerminalSubscriptionController();

    final plan = controller.replaceProject('project-a');

    expect(plan.unsubscribe, isNull);
    expect(plan.subscribe?.type, RemoteMessageType.resourceSubscribe);
    expect(plan.subscribeProjectId, 'project-a');
    expect(plan.subscribe?.payload, containsPair('projectId', 'project-a'));
    expect(plan.subscribe?.payload, isNot(contains('baseline')));
  });

  test('duplicate project subscription is skipped after commit', () {
    final controller = RemoteTerminalSubscriptionController();
    final first = controller.replaceProject('project-a');
    controller.markProjectSubscribed(first.subscribeProjectId!);

    final duplicate = controller.replaceProject('project-a');

    expect(duplicate.hasWork, isFalse);
    expect(controller.projectId, 'project-a');
  });

  test('project switch unsubscribes previous topology subscription', () {
    final controller = RemoteTerminalSubscriptionController();
    controller.markProjectSubscribed('project-a');

    final plan = controller.replaceProject('project-b');

    expect(plan.unsubscribe?.type, RemoteMessageType.resourceUnsubscribe);
    expect(plan.unsubscribe?.payload, containsPair('projectId', 'project-a'));
    expect(plan.subscribe?.type, RemoteMessageType.resourceSubscribe);
    expect(plan.subscribe?.payload, containsPair('projectId', 'project-b'));
  });

  test('tracks session subscriptions independently from project topology', () {
    final controller = RemoteTerminalSubscriptionController();
    controller.markProjectSubscribed('project-a');
    controller.markSessionSubscribed('term-a');
    controller.markSessionSubscribed('term-b');

    expect(controller.sessionCount, 2);
    expect(controller.isSessionSubscribed('term-a'), isTrue);
    expect(controller.removeSession('term-a'), isTrue);
    expect(controller.removeSession('term-a'), isFalse);
    expect(controller.projectId, 'project-a');
  });

  test('reset clears project and session subscriptions', () {
    final controller = RemoteTerminalSubscriptionController();
    controller.markProjectSubscribed('project-a');
    controller.markSessionSubscribed('term-a');

    controller.reset();

    expect(controller.projectId, isNull);
    expect(controller.sessionCount, 0);
  });
}
