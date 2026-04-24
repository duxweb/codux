import Foundation

func normalizedComparablePath(_ path: String?) -> String? {
    guard let path = normalizedNonEmptyString(path) else {
        return nil
    }
    let expanded = (path as NSString).expandingTildeInPath
    return URL(fileURLWithPath: expanded, isDirectory: true)
        .standardizedFileURL
        .resolvingSymlinksInPath()
        .path
}

func pathsEquivalent(_ lhs: String?, _ rhs: String?) -> Bool {
    guard let lhs = normalizedComparablePath(lhs),
          let rhs = normalizedComparablePath(rhs) else {
        return false
    }
    return lhs == rhs
}
