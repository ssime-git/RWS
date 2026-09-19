import XCTest
@testable import RWSApp

final class StartupTests: XCTestCase {
    func testDiscoveryPrefersExistingDefaultAndDeduplicatesAncestorSources() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let local = root.appendingPathComponent(".rws-local/config.json")
        let standard = root.appendingPathComponent("user/config.json")
        try FileManager.default.createDirectory(at: local.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: local)
        let app = root.appendingPathComponent("dist/development/RWS.app")
        XCTAssertEqual(StartupDiscovery.configurations(defaultURL: standard, remembered: local, appURL: app, home: root), [local])
        try FileManager.default.createDirectory(at: standard.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data("{}".utf8).write(to: standard)
        XCTAssertEqual(StartupDiscovery.configurations(defaultURL: standard, remembered: local, appURL: app, home: root), [standard])
    }

    func testAmbiguousCandidatesAreNotSilentlyChosen() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let local = root.appendingPathComponent(".rws-local/config.json")
        let legacy = root.appendingPathComponent(".config/rws/config.json")
        for url in [local, legacy] {
            try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data("{}".utf8).write(to: url)
        }
        let candidates = StartupDiscovery.configurations(defaultURL: root.appendingPathComponent("default.json"), remembered: nil, appURL: root.appendingPathComponent("dist/RWS.app"), home: root)
        XCTAssertEqual(Set(candidates), Set([local, legacy]))
    }

    func testPrerequisitesDistinguishMissingBrokenAndUnsupported() {
        XCTAssertNotNil(MountPrerequisites.problem(macFUSEPresent: false, sshfsVersion: "3.7.5-rws-fskit3", fskit: true))
        XCTAssertNotNil(MountPrerequisites.problem(macFUSEPresent: true, sshfsVersion: nil, fskit: true))
        XCTAssertNotNil(MountPrerequisites.problem(macFUSEPresent: true, sshfsVersion: "SSHFS 3.7.5", fskit: true))
        XCTAssertNil(MountPrerequisites.problem(macFUSEPresent: true, sshfsVersion: "SSHFS version 3.7.5-rws-fskit3", fskit: true))
    }
}
