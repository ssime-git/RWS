import XCTest
@testable import RWSApp

final class MountLifecycleLiveTests: XCTestCase {
    /// Explicitly opt-in: this disconnects/reconnects ONLY the selected workspace.
    func testReconnectReplacesSidebarBookmark() async throws {
        let env = ProcessInfo.processInfo.environment
        guard env["RWS_TEST_MOUNT_CYCLE"] == "1", let cli = env["RWS_TEST_CLI"],
              let config = env["RWS_TEST_CONFIG"], let name = env["RWS_TEST_WORKSPACE"] else {
            throw XCTSkip("Requires explicit authorization for a mount cycle")
        }
        let cliURL = URL(fileURLWithPath: cli)
        let configURL = URL(fileURLWithPath: config)
        let configuration = try AppConfiguration.decode(Data(contentsOf: configURL))
        let workspace = try XCTUnwrap(configuration.workspaces.first { $0.name == name })
        let folder = URL(fileURLWithPath: workspace.mountRoot, isDirectory: true)
        let runner = ProcessRunner()
        let connected = try await runner.run(executable: cliURL, arguments: CLICommand.connect(config: configURL, workspace: name).arguments)
        XCTAssertEqual(connected.exitCode, 0, connected.stdout + connected.stderr)
        try FinderSidebar.pin(folder)
        let disconnected = try await runner.run(executable: cliURL, arguments: CLICommand.disconnect(config: configURL, workspace: name).arguments)
        XCTAssertEqual(disconnected.exitCode, 0, disconnected.stdout + disconnected.stderr)
        guard disconnected.exitCode == 0 else { return }
        // Capture cleanup errors without skipping the reconnect that restores the workspace.
        var cleanupError: Error?
        do { try FinderSidebar.remove(folder) } catch { cleanupError = error }
        let reconnected = try await runner.run(executable: cliURL, arguments: CLICommand.connect(config: configURL, workspace: name).arguments)
        XCTAssertEqual(reconnected.exitCode, 0, reconnected.stdout + reconnected.stderr)
        try FinderSidebar.pin(folder)
        XCTAssertNil(cleanupError)
        XCTAssertEqual(try FinderSidebar.paths().filter { $0 == folder.path }.count, 1)
        let status = try await runner.run(executable: cliURL, arguments: CLICommand.status(config: configURL, workspace: name).arguments)
        XCTAssertTrue(status.stdout.contains("connected (verified RWS mount)"), status.stdout + status.stderr)
    }
}
