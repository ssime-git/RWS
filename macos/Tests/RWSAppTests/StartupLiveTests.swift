import XCTest
@testable import RWSApp

final class StartupLiveTests: XCTestCase {
    /// Explicit opt-in, read-only check against an installed CLI and existing config.
    func testReadOnlyInstalledConfigurationAndDependencies() async throws {
        let env = ProcessInfo.processInfo.environment
        guard let cliPath = env["RWS_TEST_CLI"], let configPath = env["RWS_TEST_CONFIG"] else {
            throw XCTSkip("Set RWS_TEST_CLI and RWS_TEST_CONFIG for read-only local acceptance")
        }
        let configURL = URL(fileURLWithPath: configPath)
        let original = try Data(contentsOf: configURL)
        let result = try await ProcessRunner().run(executable: URL(fileURLWithPath: cliPath), arguments: CLICommand.list(config: configURL).arguments)
        XCTAssertEqual(result.exitCode, 0, result.stderr)
        XCTAssertFalse(result.outputWasTruncated)
        let configuration = try AppConfiguration.decode(Data(result.stdout.utf8))
        XCTAssertFalse(configuration.workspaces.isEmpty)
        let sshfs = try XCTUnwrap(configuration.mount.sshfs)
        let version = try await ProcessRunner(timeout: .seconds(5)).run(executable: URL(fileURLWithPath: sshfs), arguments: ["--version"])
        XCTAssertEqual(version.exitCode, 0)
        XCTAssertNil(MountPrerequisites.problem(macFUSEPresent: FileManager.default.fileExists(atPath: "/Library/Filesystems/macfuse.fs/Contents/Info.plist") && FileManager.default.fileExists(atPath: "/usr/local/lib/libfuse3.4.dylib"), sshfsVersion: version.stdout + version.stderr, fskit: configuration.mount.fskit))
        let status = try await ProcessRunner().run(executable: URL(fileURLWithPath: cliPath), arguments: CLICommand.status(config: configURL, workspace: nil).arguments)
        XCTAssertEqual(status.exitCode, 0, status.stderr)
        XCTAssertEqual(try Data(contentsOf: configURL), original, "Startup checks must not rewrite personal configuration")
    }
}
