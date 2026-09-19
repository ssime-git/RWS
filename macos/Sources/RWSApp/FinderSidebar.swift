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

    static func paths() throws -> [String] { try entries(list()).compactMap(path) }

    static func pin(_ folder: URL) throws {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: folder.path, isDirectory: &isDirectory), isDirectory.boolValue else {
            throw Failure.missingFolder
        }
        let list = try list()
        let target = folder.standardizedFileURL.path
        // Insertion refreshes an existing URL as well as adding a new one.
        guard RWSInsertSidebarURL(list, folder as CFURL),
              try entries(list).contains(where: { path($0) == target }) else { throw Failure.notSaved }
    }

    // Used for cleanup of the disposable acceptance-test favorite only.
    static func remove(_ folder: URL) throws {
        let list = try list()
        for item in try entries(list) where path(item) == folder.standardizedFileURL.path {
            guard LSSharedFileListItemRemove(list, item) == noErr else { throw Failure.notSaved }
        }
    }
}
