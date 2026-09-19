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
