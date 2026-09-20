import XCTest
@testable import RWSApp

final class FinderSidebarTests: XCTestCase {
    func testAuthorizedMountedWorkspace() throws {
        guard let path = ProcessInfo.processInfo.environment["RWS_TEST_SIDEBAR_MOUNT"] else {
            throw XCTSkip("Requires a mounted workspace explicitly selected for pinning")
        }
        let folder = URL(fileURLWithPath: path, isDirectory: true)
        try FinderSidebar.pin(folder)
        try FinderSidebar.pin(folder)
        XCTAssertEqual(try FinderSidebar.paths().filter { $0 == folder.standardizedFileURL.path }.count, 1)
    }

    func testRecreatedFolderReplacesOriginalBookmark() throws {
        guard ProcessInfo.processInfo.environment["RWS_TEST_SIDEBAR"] == "1" else { throw XCTSkip("Requires local Finder") }
        let fm = FileManager.default
        let base = fm.temporaryDirectory.appendingPathComponent("RWS-bookmark-\(UUID())")
        let folder = base.appendingPathComponent("mount")
        let moved = base.appendingPathComponent("old-mount")
        try fm.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FinderSidebar.remove(folder); try? FinderSidebar.remove(moved); try? fm.removeItem(at: base) }
        let before = try FinderSidebar.paths()
        try FinderSidebar.pin(folder)
        try fm.moveItem(at: folder, to: moved)
        try fm.createDirectory(at: folder, withIntermediateDirectories: false)
        try FinderSidebar.pin(folder)
        let after = try FinderSidebar.paths()
        XCTAssertEqual(after.filter { $0 == folder.path }.count, 1)
        XCTAssertFalse(after.contains(moved.path), "Old bookmark must not survive replacement of the same mount path")
        XCTAssertEqual(after.filter { $0 != folder.path }, before)
    }

    func testMissingFolderDoesNotCreateFavorite() throws {
        XCTAssertThrowsError(try FinderSidebar.pin(URL(fileURLWithPath: "/rws-missing-\(UUID())")))
    }

    func testLivePinIsIdempotentAndPreservesOtherFavorites() throws {
        guard ProcessInfo.processInfo.environment["RWS_TEST_SIDEBAR"] == "1" else {
            throw XCTSkip("Requires explicit local Finder sidebar acceptance")
        }
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("RWS-sidebar-\(UUID())")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: folder) }
        let before = try FinderSidebar.paths()
        defer { try? FinderSidebar.remove(folder) }
        try FinderSidebar.pin(folder)
        try FinderSidebar.pin(folder)
        let after = try FinderSidebar.paths()
        XCTAssertEqual(after.filter { $0 == folder.standardizedFileURL.path }.count, 1)
        XCTAssertEqual(after.filter { $0 != folder.standardizedFileURL.path }, before)
    }
}
