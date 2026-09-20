import Foundation
import CoreServices
import SidebarBridge

/// The public shared-list API is deprecated. Keep it isolated and verify writes;
/// a future macOS rejection must be surfaced, never reported as a successful pin.
enum FinderSidebar {
    enum Failure: LocalizedError {
        case unavailable, missingFolder, notSaved
        var errorDescription: String? {
            switch self {
            case .unavailable: "La barre latérale du Finder est inaccessible."
            case .missingFolder: "Le dossier doit être monté avant son ajout au Finder."
            case .notSaved: "Le Finder n’a pas enregistré le raccourci."
            }
        }
    }

    private static func list() throws -> LSSharedFileList {
        guard let list = LSSharedFileListCreate(nil, kLSSharedFileListFavoriteItems.takeUnretainedValue(), nil) else {
            throw Failure.unavailable
        }
        return list.takeRetainedValue()
    }

    private static func entries(_ list: LSSharedFileList) throws -> [LSSharedFileListItem] {
        guard let snapshot = LSSharedFileListCopySnapshot(list, nil),
              let items = snapshot.takeRetainedValue() as? [LSSharedFileListItem] else { throw Failure.unavailable }
        return items
    }

    private static func path(_ item: LSSharedFileListItem) -> String? {
        let flags = UInt32(kLSSharedFileListNoUserInteraction | kLSSharedFileListDoNotMountVolumes)
        guard let url = LSSharedFileListItemCopyResolvedURL(item, flags, nil)?.takeRetainedValue() else { return nil }
        return (url as URL).standardizedFileURL.path
    }

    private static let managedPathKey = "io.github.ssime-git.RWS.mountPath" as CFString

    private static func matches(_ item: LSSharedFileListItem, folder: URL, legacyVolumeName: String? = nil) -> Bool {
        let target = folder.standardizedFileURL.path
        if let managed = LSSharedFileListItemCopyProperty(item, managedPathKey)?.takeRetainedValue() as? String,
           managed == target { return true }
        let resolved = path(item)
        if resolved == target { return true }
        // One-time migration of unresolved favourites made by earlier RWS builds.
        // Only the current RWS volume's exact system name qualifies, never a prefix.
        if resolved == nil, let name = legacyVolumeName, name.hasPrefix("RWS-") {
            return LSSharedFileListItemCopyDisplayName(item).takeRetainedValue() as String == name
        }
        return false
    }

    static func paths() throws -> [String] { try entries(list()).compactMap(path) }

    static func pin(_ folder: URL) throws {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: folder.path, isDirectory: &isDirectory), isDirectory.boolValue else {
            throw Failure.missingFolder
        }
        let list = try list()
        let target = folder.standardizedFileURL.path
        let volumeName = folder.path.hasPrefix("/Volumes/RWS-")
            ? try folder.resourceValues(forKeys: [.volumeNameKey]).volumeName : nil
        // A bookmark follows a filesystem identity, not just a path. Reusing its
        // entry can preserve the old FSKit mount even when the URL looks correct.
        for item in try entries(list) where matches(item, folder: folder, legacyVolumeName: volumeName) {
            guard LSSharedFileListItemRemove(list, item) == noErr else { throw Failure.notSaved }
        }
        guard RWSInsertSidebarURL(list, folder as CFURL),
              try entries(list).contains(where: { path($0) == target }) else { throw Failure.notSaved }
    }

    // Remove only this mount's managed entry, including after its volume disappears.
    static func remove(_ folder: URL, legacyVolumeName: String? = nil) throws {
        let list = try list()
        for item in try entries(list) where matches(item, folder: folder, legacyVolumeName: legacyVolumeName) {
            guard LSSharedFileListItemRemove(list, item) == noErr else { throw Failure.notSaved }
        }
    }
}
