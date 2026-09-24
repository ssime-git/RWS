import XCTest
@testable import RWSApp

final class ConfigurationTests: XCTestCase {
    func testPreservesMountIntentWhenRoundTrippingConfiguration() throws {
        let data = #"{"version":1,"workspaces":[{"name":"demo","host":"vm","remote_root":"/srv","mount_root":"/Volumes/demo"}],"mount_intent":{"demo":"paused"}}"#.data(using: .utf8)!
        let config = try AppConfiguration.decode(data)
        let encoded = try JSONEncoder().encode(config)
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
        XCTAssertEqual(object["mount_intent"] as? [String: String], ["demo": "paused"])
    }

    func testRejectsInvalidMountIntent() {
        for intent in [#"{"demo":"unknown"}"#, #"{"demo":false}"#, #"[]"#, "null"] {
            let json = #"{"version":1,"workspaces":[],"mount_intent":\#(intent)}"#
            XCTAssertThrowsError(try AppConfiguration.decode(Data(json.utf8)))
        }
    }

    func testAcceptsDurableInstallationGeneration() throws {
        let data = #"{"version":1,"workspaces":[],"mount_state_generation":"release-123"}"#.data(using: .utf8)!
        let config = try AppConfiguration.decode(data)
        XCTAssertEqual(config.mountStateGeneration, "release-123")
    }
    func testDecodesCurrentRustConfiguration() throws {
        let data = #"{"version":1,"workspaces":[{"name":"demo","host":"vm","remote_root":"/srv","mount_root":"/Volumes/demo"}],"mount":{"sshfs":"/opt/sshfs","fskit":true}}"#.data(using: .utf8)!
        let config = try AppConfiguration.decode(data)
        XCTAssertEqual(config.workspaces.first?.remoteRoot, "/srv")
        XCTAssertEqual(config.mount.sshfs, "/opt/sshfs")
    }

    func testDecodesNativeNFSConfiguration() throws {
        let data = #"{"version":1,"workspaces":[],"mount":{"nfs":true,"fskit":false}}"#.data(using: .utf8)!
        let config = try AppConfiguration.decode(data)
        XCTAssertTrue(config.mount.nfs)
        XCTAssertFalse(config.mount.fskit)
        let encoded = try JSONEncoder().encode(config)
        XCTAssertTrue(try AppConfiguration.decode(encoded).mount.nfs)
    }

    func testRejectsNFSAndFSKitTogether() {
        let data = #"{"version":1,"workspaces":[],"mount":{"nfs":true,"fskit":true}}"#.data(using: .utf8)!
        XCTAssertThrowsError(try AppConfiguration.decode(data))
    }

    func testDecodesConfigurationFromBeforeMountSettingsWereAdded() throws {
        let data = #"{"version":1,"workspaces":[]}"#.data(using: .utf8)!
        let config = try AppConfiguration.decode(data)
        XCTAssertNil(config.mount.sshfs)
        XCTAssertFalse(config.mount.fskit)
    }

    func testRejectsUnsupportedVersionAndUnknownFields() {
        XCTAssertThrowsError(try AppConfiguration.decode(#"{"version":2,"workspaces":[],"mount":{}}"#.data(using: .utf8)!))
        XCTAssertThrowsError(try AppConfiguration.decode(#"{"version":1,"workspaces":[],"mount":{},"secret":true}"#.data(using: .utf8)!))
    }

    func testImportRefusesToReplaceExistingConfiguration() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("source.json")
        let destination = root.appendingPathComponent("destination.json")
        let valid = #"{"version":1,"workspaces":[],"mount":{}}"#.data(using: .utf8)!
        try valid.write(to: source)
        try Data("existing".utf8).write(to: destination)
        XCTAssertThrowsError(try ConfigurationImporter.importConfig(from: source, to: destination))
        XCTAssertEqual(try Data(contentsOf: destination), Data("existing".utf8))
    }
}
