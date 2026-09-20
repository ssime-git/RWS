import XCTest
@testable import RWSApp

final class AgentLauncherTests: XCTestCase {
    private func run(_ script: String) throws -> (status: Int32, lines: [String]) {
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", script]
        process.standardOutput = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        return (process.terminationStatus,
                String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init))
    }

    func testLauncherCannotInterpretExecutableOrPathsAsShellCode() throws {
        let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: tmp) }
        let fakeRWS = tmp.appendingPathComponent("rws ' binary")
        try Data("#!/bin/sh\nprintf '%s\\n' \"$@\"\n".utf8).write(to: fakeRWS)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fakeRWS.path)
        let config = tmp.appendingPathComponent("config ' file")
        let command = "claude; echo LOCAL"
        let script = AgentLauncher.script(binary: fakeRWS, config: config, workspace: "demo ' space",
                                          executable: command, mode: .remote)
        let result = try run(script)
        XCTAssertEqual(result.status, 0)
        XCTAssertEqual(result.lines,
                       ["--config", config.path, "agent", "--workspace", "demo ' space", "--", command])
    }

    func testRemoteScriptForcesRemoteAutoModeForNestedShells() {
        let script = AgentLauncher.script(binary: URL(fileURLWithPath: "/opt/rws"),
                                          config: URL(fileURLWithPath: "/cfg.json"),
                                          workspace: "demo", executable: "claude", mode: .remote)
        XCTAssertTrue(script.contains("RWS_AUTO_MODE=remote"), script)
        XCTAssertTrue(script.contains("export RWS_AUTO_MODE"), script)
    }

    func testLocalScriptRunsExecutableInMountWithLocalAutoMode() throws {
        let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let mount = tmp.appendingPathComponent("RWS-demo ' vol")
        try FileManager.default.createDirectory(at: mount, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: tmp) }
        let agent = tmp.appendingPathComponent("agent ' cli")
        try Data("#!/bin/sh\npwd\nprintf 'MODE:%s\\n' \"$RWS_AUTO_MODE\"\n".utf8).write(to: agent)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: agent.path)
        let script = AgentLauncher.script(binary: URL(fileURLWithPath: "/opt/unused rws"),
                                          config: URL(fileURLWithPath: "/cfg.json"),
                                          workspace: "demo", executable: agent.path,
                                          mode: .local(mountRoot: mount.path))
        let result = try run(script)
        XCTAssertEqual(result.status, 0)
        XCTAssertEqual(result.lines, [mount.resolvingSymlinksInPath().path, "MODE:local"])
        XCTAssertFalse(script.contains("agent --workspace"), script)
    }

    func testLocalScriptFailsInsteadOfRunningOutsideAMissingMount() throws {
        let script = AgentLauncher.script(binary: URL(fileURLWithPath: "/opt/rws"),
                                          config: URL(fileURLWithPath: "/cfg.json"),
                                          workspace: "demo", executable: "/bin/pwd",
                                          mode: .local(mountRoot: "/nonexistent/rws-mount"))
        let result = try run(script)
        XCTAssertNotEqual(result.status, 0)
        XCTAssertFalse(result.lines.contains("/"), "must not run in another directory")
    }
}
