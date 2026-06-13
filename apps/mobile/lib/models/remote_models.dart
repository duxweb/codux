class PairingPayload {
  const PairingPayload({
    required this.code,
    required this.secret,
    required this.hostPublicKey,
    required this.devicePrivateKey,
    required this.devicePublicKey,
    required this.transports,
    this.cryptoVersion = 1,
    this.hostName,
    this.hostId,
    this.pairingId,
  });
  final String code;
  final String secret;
  final String hostPublicKey;
  final String devicePrivateKey;
  final String devicePublicKey;
  final List<RemoteTransportCandidate> transports;
  final int cryptoVersion;
  final String? hostName;
  final String? hostId;
  final String? pairingId;

  RemoteTransportCandidate? transportByKind(String kind) {
    for (final candidate in transports) {
      if (candidate.kind == kind && candidate.url.trim().isNotEmpty) {
        return candidate;
      }
    }
    return null;
  }
}

abstract final class RemoteTransportKind {
  static const websocketRelay = 'websocketRelay';
  static const webRtc = 'webRtc';
}

class RemoteTransportCandidate {
  const RemoteTransportCandidate({
    required this.kind,
    this.role,
    this.url = '',
    this.iceServers = const [],
  });

  final String kind;
  final String? role;
  final String url;
  final List<RemoteIceServer> iceServers;

  factory RemoteTransportCandidate.fromJson(Map<String, dynamic> json) =>
      RemoteTransportCandidate(
        kind: '${json['kind'] ?? json['transport'] ?? ''}',
        role: json['role']?.toString(),
        url: '${json['url'] ?? ''}',
        iceServers: remoteIceServersFromJson(json['iceServers']),
      );

  Map<String, dynamic> toJson() => {
    'kind': kind,
    if (role != null) 'role': role,
    if (url.isNotEmpty) 'url': url,
    if (iceServers.isNotEmpty)
      'iceServers': iceServers.map((item) => item.toJson()).toList(),
  };
}

class RemoteIceServer {
  const RemoteIceServer({required this.urls});

  final List<String> urls;

  factory RemoteIceServer.fromJson(Map<String, dynamic> json) {
    final value = json['urls'];
    return RemoteIceServer(
      urls: value is List
          ? value
                .map((item) => '$item')
                .where((item) => item.trim().isNotEmpty)
                .toList()
          : ['${value ?? ''}'].where((item) => item.trim().isNotEmpty).toList(),
    );
  }

  Map<String, dynamic> toJson() => {'urls': urls};
}

List<RemoteIceServer> remoteIceServersFromJson(Object? value) {
  if (value is List) {
    return value
        .whereType<Map>()
        .map(
          (item) => RemoteIceServer.fromJson(Map<String, dynamic>.from(item)),
        )
        .where((item) => item.urls.isNotEmpty)
        .toList();
  }
  return const [];
}

List<RemoteTransportCandidate> remoteTransportCandidatesFromJson(
  Object? value,
) {
  if (value is List) {
    return value
        .whereType<Map>()
        .map(
          (item) => RemoteTransportCandidate.fromJson(
            Map<String, dynamic>.from(item),
          ),
        )
        .where((item) => item.kind.trim().isNotEmpty)
        .toList();
  }
  return const [];
}

class StoredDevice {
  const StoredDevice({
    required this.server,
    required this.hostId,
    required this.deviceId,
    required this.token,
    required this.name,
    this.hostPublicKey = '',
    this.devicePrivateKey = '',
    this.devicePublicKey = '',
    this.cryptoVersion = 0,
    this.hostName,
    this.transports = const [],
  });
  final String server;
  final String hostId;
  final String deviceId;
  final String token;
  final String name;
  final String hostPublicKey;
  final String devicePrivateKey;
  final String devicePublicKey;
  final int cryptoVersion;
  final String? hostName;
  final List<RemoteTransportCandidate> transports;

  RemoteTransportCandidate? transportByKind(String kind) {
    for (final candidate in transports) {
      if (candidate.kind == kind && candidate.url.trim().isNotEmpty) {
        return candidate;
      }
    }
    return null;
  }

  StoredDevice copyWith({
    String? server,
    String? hostId,
    String? deviceId,
    String? token,
    String? name,
    String? hostPublicKey,
    String? devicePrivateKey,
    String? devicePublicKey,
    int? cryptoVersion,
    String? hostName,
    List<RemoteTransportCandidate>? transports,
  }) {
    return StoredDevice(
      server: server ?? this.server,
      hostId: hostId ?? this.hostId,
      deviceId: deviceId ?? this.deviceId,
      token: token ?? this.token,
      name: name ?? this.name,
      hostPublicKey: hostPublicKey ?? this.hostPublicKey,
      devicePrivateKey: devicePrivateKey ?? this.devicePrivateKey,
      devicePublicKey: devicePublicKey ?? this.devicePublicKey,
      cryptoVersion: cryptoVersion ?? this.cryptoVersion,
      hostName: hostName ?? this.hostName,
      transports: transports ?? this.transports,
    );
  }

  factory StoredDevice.fromJson(Map<String, dynamic> json) {
    final transports = remoteTransportCandidatesFromJson(json['transports']);
    return StoredDevice(
      server: '${json['server'] ?? ''}',
      hostId: '${json['hostId'] ?? ''}',
      deviceId: '${json['deviceId'] ?? ''}',
      token: '${json['token'] ?? ''}',
      name: '${json['name'] ?? ''}',
      hostPublicKey: '${json['hostPublicKey'] ?? ''}',
      devicePrivateKey: '${json['devicePrivateKey'] ?? ''}',
      devicePublicKey: '${json['devicePublicKey'] ?? ''}',
      cryptoVersion: json['cryptoVersion'] is num
          ? (json['cryptoVersion'] as num).toInt()
          : int.tryParse('${json['cryptoVersion'] ?? ''}') ?? 0,
      hostName: json['hostName'] == null ? null : '${json['hostName']}',
      transports: transports,
    );
  }

  Map<String, dynamic> toJson() => {
    if (server.isNotEmpty) 'server': server,
    'hostId': hostId,
    'deviceId': deviceId,
    'token': token,
    'name': name,
    if (hostPublicKey.isNotEmpty) 'hostPublicKey': hostPublicKey,
    if (devicePrivateKey.isNotEmpty) 'devicePrivateKey': devicePrivateKey,
    if (devicePublicKey.isNotEmpty) 'devicePublicKey': devicePublicKey,
    if (cryptoVersion > 0) 'cryptoVersion': cryptoVersion,
    if (hostName != null) 'hostName': hostName,
    'transports': transports.map((item) => item.toJson()).toList(),
  };
}

class RelayEnvelope {
  const RelayEnvelope({
    required this.type,
    this.id,
    this.hostId,
    this.deviceId,
    this.sessionId,
    this.seq,
    this.payload,
    this.error,
    this.at,
  });
  final String type;
  final String? id;
  final String? hostId;
  final String? deviceId;
  final String? sessionId;
  final int? seq;
  final Object? payload;
  final String? error;
  final int? at;

  factory RelayEnvelope.fromJson(Map<String, dynamic> json) => RelayEnvelope(
    type: '${json['type'] ?? ''}',
    id: json['id']?.toString(),
    hostId: json['hostId']?.toString(),
    deviceId: json['deviceId']?.toString(),
    sessionId: json['sessionId']?.toString(),
    seq: json['seq'] is num ? (json['seq'] as num).toInt() : null,
    payload: json['payload'],
    error: json['error']?.toString(),
    at: json['at'] is num ? (json['at'] as num).toInt() : null,
  );

  Map<String, dynamic> toJson() => {
    'type': type,
    if (id != null) 'id': id,
    if (hostId != null) 'hostId': hostId,
    if (deviceId != null) 'deviceId': deviceId,
    if (sessionId != null) 'sessionId': sessionId,
    if (seq != null) 'seq': seq,
    if (payload != null) 'payload': payload,
    if (error != null) 'error': error,
    if (at != null) 'at': at,
  };

  RelayEnvelope copyWith({
    String? type,
    String? id,
    String? hostId,
    String? deviceId,
    String? sessionId,
    int? seq,
    Object? payload,
    String? error,
    int? at,
  }) => RelayEnvelope(
    type: type ?? this.type,
    id: id ?? this.id,
    hostId: hostId ?? this.hostId,
    deviceId: deviceId ?? this.deviceId,
    sessionId: sessionId ?? this.sessionId,
    seq: seq ?? this.seq,
    payload: payload ?? this.payload,
    error: error ?? this.error,
    at: at ?? this.at,
  );
}

class ProjectInfo {
  const ProjectInfo({required this.id, required this.name, this.path});
  final String id;
  final String name;
  final String? path;

  factory ProjectInfo.fromJson(Map<String, dynamic> json) => ProjectInfo(
    id: '${json['id'] ?? ''}',
    name: '${json['name'] ?? 'Project'}',
    path: json['path']?.toString(),
  );

  Map<String, dynamic> toJson() => {
    'id': id,
    'name': name,
    if (path != null) 'path': path,
  };
}

class TerminalInfo {
  const TerminalInfo({
    required this.id,
    required this.title,
    required this.projectId,
    this.layoutKind = 'split',
    this.worktreeId,
    this.layoutOrder,
    this.cols,
    this.rows,
    this.status,
    this.createdAt,
    this.bufferCharacters,
  });
  final String id;
  final String title;
  final String projectId;
  final String layoutKind;
  final String? worktreeId;
  final int? layoutOrder;
  final int? cols;
  final int? rows;
  final String? status;
  final String? createdAt;
  final int? bufferCharacters;

  factory TerminalInfo.fromJson(Map<String, dynamic> json) => TerminalInfo(
    id: '${json['id'] ?? ''}',
    title: '${json['title'] ?? 'Terminal'}',
    projectId: '${json['projectId'] ?? ''}',
    layoutKind: '${json['layoutKind'] ?? 'split'}',
    worktreeId: json['worktreeId']?.toString(),
    layoutOrder: json['layoutOrder'] is num
        ? (json['layoutOrder'] as num).toInt()
        : int.tryParse('${json['layoutOrder'] ?? ''}'),
    cols: json['cols'] is num
        ? (json['cols'] as num).toInt()
        : int.tryParse('${json['cols'] ?? ''}'),
    rows: json['rows'] is num
        ? (json['rows'] as num).toInt()
        : int.tryParse('${json['rows'] ?? ''}'),
    status: json['status']?.toString(),
    createdAt: json['createdAt']?.toString(),
    bufferCharacters: json['bufferCharacters'] is num
        ? (json['bufferCharacters'] as num).toInt()
        : int.tryParse('${json['bufferCharacters'] ?? ''}'),
  );
}

class RemoteWorktreeInfo {
  const RemoteWorktreeInfo({
    required this.id,
    required this.projectId,
    required this.name,
    required this.branch,
    required this.path,
    required this.status,
    required this.isDefault,
    required this.exists,
    this.baseBranch,
    this.changes = 0,
    this.incoming = 0,
    this.outgoing = 0,
    this.additions = 0,
    this.deletions = 0,
  });

  final String id;
  final String projectId;
  final String name;
  final String branch;
  final String path;
  final String status;
  final bool isDefault;
  final bool exists;
  final String? baseBranch;
  final int changes;
  final int incoming;
  final int outgoing;
  final int additions;
  final int deletions;

  factory RemoteWorktreeInfo.fromJson(Map<String, dynamic> json) {
    final gitSummary = json['gitSummary'] is Map
        ? Map<String, dynamic>.from(json['gitSummary'] as Map)
        : const <String, dynamic>{};
    return RemoteWorktreeInfo(
      id: '${json['id'] ?? ''}',
      projectId: '${json['projectId'] ?? ''}',
      name: '${json['name'] ?? ''}',
      branch: '${json['branch'] ?? ''}',
      path: '${json['path'] ?? ''}',
      status: '${json['status'] ?? ''}',
      isDefault: json['isDefault'] == true,
      exists: json['exists'] != false,
      baseBranch: json['baseBranch']?.toString(),
      changes: _intValue(gitSummary['changes']) ?? 0,
      incoming: _intValue(gitSummary['incoming']) ?? 0,
      outgoing: _intValue(gitSummary['outgoing']) ?? 0,
      additions: _intValue(gitSummary['additions']) ?? 0,
      deletions: _intValue(gitSummary['deletions']) ?? 0,
    );
  }

  Map<String, dynamic> toJson() => {
    'id': id,
    'projectId': projectId,
    'name': name,
    'branch': branch,
    'path': path,
    'status': status,
    'isDefault': isDefault,
    'exists': exists,
    if (baseBranch != null) 'baseBranch': baseBranch,
    'changes': changes,
    'incoming': incoming,
    'outgoing': outgoing,
    'additions': additions,
    'deletions': deletions,
  };

  RemoteWorktreeInfo copyWith({String? baseBranch}) {
    return RemoteWorktreeInfo(
      id: id,
      projectId: projectId,
      name: name,
      branch: branch,
      path: path,
      status: status,
      isDefault: isDefault,
      exists: exists,
      baseBranch: baseBranch ?? this.baseBranch,
      changes: changes,
      incoming: incoming,
      outgoing: outgoing,
      additions: additions,
      deletions: deletions,
    );
  }
}

class RemoteFileEntry {
  const RemoteFileEntry({
    required this.name,
    required this.path,
    required this.isDirectory,
  });

  final String name;
  final String path;
  final bool isDirectory;

  factory RemoteFileEntry.fromJson(Map<String, dynamic> json) =>
      RemoteFileEntry(
        name: '${json['name'] ?? ''}',
        path: '${json['path'] ?? ''}',
        isDirectory: json['isDirectory'] == true,
      );
}

class RemoteGitStatusInfo {
  const RemoteGitStatusInfo({
    required this.projectId,
    required this.projectPath,
    required this.branch,
    this.upstream,
    this.ahead = 0,
    this.behind = 0,
    this.staged = 0,
    this.unstaged = 0,
    this.untracked = 0,
    this.changes = 0,
    this.isRepository = false,
    this.error,
    this.changedFiles = const [],
  });

  final String projectId;
  final String projectPath;
  final String branch;
  final String? upstream;
  final int ahead;
  final int behind;
  final int staged;
  final int unstaged;
  final int untracked;
  final int changes;
  final bool isRepository;
  final String? error;
  final List<RemoteGitFileStatus> changedFiles;

  factory RemoteGitStatusInfo.fromJson(Map<String, dynamic> json) =>
      RemoteGitStatusInfo(
        projectId: '${json['projectId'] ?? ''}',
        projectPath: '${json['projectPath'] ?? ''}',
        branch: '${json['branch'] ?? ''}',
        upstream: json['upstream']?.toString(),
        ahead: _intValue(json['ahead']) ?? 0,
        behind: _intValue(json['behind']) ?? 0,
        staged: _intValue(json['staged']) ?? 0,
        unstaged: _intValue(json['unstaged']) ?? 0,
        untracked: _intValue(json['untracked']) ?? 0,
        changes: _intValue(json['changes']) ?? 0,
        isRepository: json['isRepository'] == true,
        error: json['error']?.toString(),
        changedFiles: (json['changedFiles'] as List<dynamic>? ?? [])
            .whereType<Map>()
            .map(
              (item) =>
                  RemoteGitFileStatus.fromJson(Map<String, dynamic>.from(item)),
            )
            .toList(),
      );

  Map<String, dynamic> toJson() => {
    'projectId': projectId,
    'projectPath': projectPath,
    'branch': branch,
    if (upstream != null) 'upstream': upstream,
    'ahead': ahead,
    'behind': behind,
    'staged': staged,
    'unstaged': unstaged,
    'untracked': untracked,
    'changes': changes,
    'isRepository': isRepository,
    if (error != null) 'error': error,
    'changedFiles': changedFiles.map((item) => item.toJson()).toList(),
  };
}

class RemoteGitFileStatus {
  const RemoteGitFileStatus({
    required this.path,
    required this.indexStatus,
    required this.worktreeStatus,
  });

  final String path;
  final String indexStatus;
  final String worktreeStatus;

  factory RemoteGitFileStatus.fromJson(Map<String, dynamic> json) =>
      RemoteGitFileStatus(
        path: '${json['path'] ?? ''}',
        indexStatus: '${json['indexStatus'] ?? ''}',
        worktreeStatus: '${json['worktreeStatus'] ?? ''}',
      );

  Map<String, dynamic> toJson() => {
    'path': path,
    'indexStatus': indexStatus,
    'worktreeStatus': worktreeStatus,
  };
}

class AIStatsInfo {
  const AIStatsInfo({
    required this.projectName,
    required this.todayTokens,
    required this.totalTokens,
    required this.currentSessionTokens,
    required this.requestCount,
    this.currentSessionCachedInputTokens = 0,
    this.projectCachedInputTokens = 0,
    this.todayCachedInputTokens = 0,
    this.currentTool,
    this.currentModel,
    this.contextUsagePercent,
    this.updatedAt,
    this.currentSessions = const [],
    this.todayTimeBuckets = const [],
    this.heatmap = const [],
    this.toolBreakdown = const [],
    this.modelBreakdown = const [],
  });

  final String projectName;
  final int todayTokens;
  final int totalTokens;
  final int currentSessionTokens;
  final int currentSessionCachedInputTokens;
  final int projectCachedInputTokens;
  final int todayCachedInputTokens;
  final int requestCount;
  final String? currentTool;
  final String? currentModel;
  final double? contextUsagePercent;
  final String? updatedAt;
  final List<AIStatsSessionInfo> currentSessions;
  final List<AIStatsTimeBucket> todayTimeBuckets;
  final List<AIStatsHeatmapDay> heatmap;
  final List<AIStatsBreakdownItem> toolBreakdown;
  final List<AIStatsBreakdownItem> modelBreakdown;

  factory AIStatsInfo.fromJson(Map<String, dynamic> json) => AIStatsInfo(
    projectName: '${json['projectName'] ?? 'Project'}',
    todayTokens:
        _intValue(json['todayTotalTokens']) ??
        _intValue(json['todayTokens']) ??
        0,
    totalTokens:
        _intValue(json['projectTotalTokens']) ??
        _intValue(json['totalTokens']) ??
        0,
    currentSessionTokens: _intValue(json['currentSessionTokens']) ?? 0,
    currentSessionCachedInputTokens:
        _intValue(json['currentSessionCachedInputTokens']) ?? 0,
    projectCachedInputTokens: _intValue(json['projectCachedInputTokens']) ?? 0,
    todayCachedInputTokens: _intValue(json['todayCachedInputTokens']) ?? 0,
    requestCount: _intValue(json['requestCount']) ?? 0,
    currentTool: json['currentTool']?.toString(),
    currentModel: json['currentModel']?.toString(),
    contextUsagePercent: _doubleValue(json['contextUsagePercent']),
    updatedAt: json['updatedAt']?.toString(),
    currentSessions: _listOf(
      json['currentSessions'],
      AIStatsSessionInfo.fromJson,
    ),
    todayTimeBuckets: _listOf(
      json['todayTimeBuckets'],
      AIStatsTimeBucket.fromJson,
    ),
    heatmap: _listOf(json['heatmap'], AIStatsHeatmapDay.fromJson),
    toolBreakdown: _listOf(
      json['toolBreakdown'],
      AIStatsBreakdownItem.fromJson,
    ),
    modelBreakdown: _listOf(
      json['modelBreakdown'],
      AIStatsBreakdownItem.fromJson,
    ),
  );
}

class AIStatsSessionInfo {
  const AIStatsSessionInfo({
    required this.sessionId,
    required this.title,
    required this.totalTokens,
    this.cachedInputTokens = 0,
    this.tool,
    this.model,
    this.status,
    this.isRunning = false,
  });

  final String sessionId;
  final String title;
  final int totalTokens;
  final int cachedInputTokens;
  final String? tool;
  final String? model;
  final String? status;
  final bool isRunning;

  factory AIStatsSessionInfo.fromJson(Map<String, dynamic> json) =>
      AIStatsSessionInfo(
        sessionId: '${json['sessionId'] ?? ''}',
        title: '${json['title'] ?? 'Session'}',
        totalTokens: _intValue(json['totalTokens']) ?? 0,
        cachedInputTokens: _intValue(json['cachedInputTokens']) ?? 0,
        tool: json['tool']?.toString(),
        model: json['model']?.toString(),
        status: json['status']?.toString(),
        isRunning: json['isRunning'] == true,
      );
}

class AIStatsTimeBucket {
  const AIStatsTimeBucket({
    required this.start,
    required this.totalTokens,
    this.cachedInputTokens = 0,
    this.requestCount = 0,
  });

  final String start;
  final int totalTokens;
  final int cachedInputTokens;
  final int requestCount;

  factory AIStatsTimeBucket.fromJson(Map<String, dynamic> json) =>
      AIStatsTimeBucket(
        start: '${json['start'] ?? ''}',
        totalTokens: _intValue(json['totalTokens']) ?? 0,
        cachedInputTokens: _intValue(json['cachedInputTokens']) ?? 0,
        requestCount: _intValue(json['requestCount']) ?? 0,
      );
}

class AIStatsHeatmapDay {
  const AIStatsHeatmapDay({
    required this.day,
    required this.totalTokens,
    this.cachedInputTokens = 0,
    this.requestCount = 0,
  });

  final String day;
  final int totalTokens;
  final int cachedInputTokens;
  final int requestCount;

  factory AIStatsHeatmapDay.fromJson(Map<String, dynamic> json) =>
      AIStatsHeatmapDay(
        day: '${json['day'] ?? ''}',
        totalTokens: _intValue(json['totalTokens']) ?? 0,
        cachedInputTokens: _intValue(json['cachedInputTokens']) ?? 0,
        requestCount: _intValue(json['requestCount']) ?? 0,
      );
}

class AIStatsBreakdownItem {
  const AIStatsBreakdownItem({
    required this.key,
    required this.totalTokens,
    this.cachedInputTokens = 0,
    this.requestCount = 0,
  });

  final String key;
  final int totalTokens;
  final int cachedInputTokens;
  final int requestCount;

  factory AIStatsBreakdownItem.fromJson(Map<String, dynamic> json) =>
      AIStatsBreakdownItem(
        key: '${json['key'] ?? '-'}',
        totalTokens: _intValue(json['totalTokens']) ?? 0,
        cachedInputTokens: _intValue(json['cachedInputTokens']) ?? 0,
        requestCount: _intValue(json['requestCount']) ?? 0,
      );
}

int? _intValue(Object? value) => value is num ? value.toInt() : null;
double? _doubleValue(Object? value) => value is num ? value.toDouble() : null;

List<T> _listOf<T>(Object? value, T Function(Map<String, dynamic>) mapper) {
  if (value is! List) return const [];
  return value
      .whereType<Map>()
      .map((item) => mapper(Map<String, dynamic>.from(item)))
      .toList(growable: false);
}

class MobileSettings {
  static const defaultAppTextScale = 1.0;
  static const defaultTerminalFontSize = 12.0;
  static const standardTerminalFontSize = 14.0;
  static const List<double> appTextScaleSteps = [0.875, 1.0, 1.125];
  static const List<double> terminalFontSizeSteps = [
    10.0,
    12.0,
    14.0,
    16.0,
    18.0,
  ];

  const MobileSettings({
    required this.localName,
    this.accentId = 'cyan',
    this.localeId = 'system',
    this.logLevel = 'info',
    this.appTextScale = defaultAppTextScale,
    this.terminalFontSize = defaultTerminalFontSize,
  });
  final String localName;
  final String accentId;
  final String localeId;
  final String logLevel;
  final double appTextScale;
  final double terminalFontSize;

  MobileSettings copyWith({
    String? localName,
    String? accentId,
    String? localeId,
    String? logLevel,
    double? appTextScale,
    double? terminalFontSize,
  }) {
    return MobileSettings(
      localName: localName ?? this.localName,
      accentId: accentId ?? this.accentId,
      localeId: localeId ?? this.localeId,
      logLevel: logLevel ?? this.logLevel,
      appTextScale: appTextScale ?? this.appTextScale,
      terminalFontSize: terminalFontSize ?? this.terminalFontSize,
    );
  }

  factory MobileSettings.fromJson(Map<String, dynamic> json) {
    final appTextScale = json['appTextScale'] is num
        ? (json['appTextScale'] as num).toDouble()
        : double.tryParse('${json['appTextScale'] ?? ''}');
    final terminalFontSize = json['terminalFontSize'] is num
        ? (json['terminalFontSize'] as num).toDouble()
        : double.tryParse('${json['terminalFontSize'] ?? ''}');
    return MobileSettings(
      localName: '${json['localName'] ?? ''}',
      accentId: '${json['accentId'] ?? 'cyan'}',
      localeId: '${json['localeId'] ?? 'system'}',
      logLevel: '${json['logLevel'] ?? 'info'}',
      appTextScale: _nearestFontStep(
        appTextScale,
        MobileSettings.appTextScaleSteps,
        MobileSettings.defaultAppTextScale,
      ),
      terminalFontSize: _nearestFontStep(
        terminalFontSize,
        MobileSettings.terminalFontSizeSteps,
        MobileSettings.defaultTerminalFontSize,
      ),
    );
  }

  Map<String, dynamic> toJson() => {
    'localName': localName,
    'accentId': accentId,
    'localeId': localeId,
    'logLevel': logLevel,
    'appTextScale': appTextScale,
    'terminalFontSize': terminalFontSize,
  };
}

double _nearestFontStep(double? value, List<double> steps, double fallback) {
  if (value == null || steps.isEmpty) return fallback;
  return steps.reduce(
    (best, item) => (item - value).abs() < (best - value).abs() ? item : best,
  );
}
