import '../models/remote_models.dart';
import 'remote_transport.dart';

typedef RemoteSendErrorHandler = void Function(Object error);
typedef RemoteSendResultHandler =
    void Function(RelayEnvelope message, RemoteEnvelopeSendResult result);

enum RemoteEnvelopeSendResult { delivered, droppedWhileDisconnected, rejected }

class RemoteEnvelopeSendQueue {
  int _seq = 0;
  Future<void> _chain = Future<void>.value();

  void reset({int? seed}) {
    _seq = seed ?? 0;
    _chain = Future<void>.value();
  }

  Future<void> send({
    required RelayEnvelope message,
    required RemoteTransport transport,
    required bool Function() connected,
    StoredDevice? activeDevice,
    bool terminalStream = false,
    RemoteSendErrorHandler? onError,
    RemoteSendResultHandler? onResult,
  }) {
    final seq = activeDevice == null ? null : ++_seq;
    final previous = _chain.catchError((_) {});
    final task = previous
        .then((_) async {
          if (!connected()) {
            onResult?.call(
              message,
              RemoteEnvelopeSendResult.droppedWhileDisconnected,
            );
            return;
          }
          final outgoing = _attachDeviceIdentity(message, activeDevice, seq);
          final envelope = outgoing.toJson();
          late final bool sent;
          try {
            sent = terminalStream
                ? await transport.sendTerminal(envelope)
                : await transport.send(envelope);
          } catch (error) {
            onResult?.call(message, RemoteEnvelopeSendResult.rejected);
            onError?.call(error);
            return;
          }
          onResult?.call(
            message,
            sent
                ? RemoteEnvelopeSendResult.delivered
                : RemoteEnvelopeSendResult.rejected,
          );
        })
        .catchError((Object error) {
          onError?.call(error);
        });
    _chain = task;
    return task;
  }

  RelayEnvelope _attachDeviceIdentity(
    RelayEnvelope message,
    StoredDevice? activeDevice,
    int? seq,
  ) {
    if (activeDevice == null) {
      return seq == null ? message : message.copyWith(seq: seq);
    }
    return message.copyWith(
      hostId: activeDevice.hostId,
      deviceId: activeDevice.deviceId,
      seq: seq,
    );
  }
}
