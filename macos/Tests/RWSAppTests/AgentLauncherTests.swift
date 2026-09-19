import XCTest
@testable import RWSApp

final class AgentLauncherTests: XCTestCase {
    func testLauncherCannotInterpretExecutableOrPathsAsShellCode() throws {
        let tmp = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: tmp, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: tmp) }
        let fakeRWS = tmp.appendingPathComponent("rws ' binary")
        try Data("#!/bin/sh\nprintf '%s\\n' \"$@\"\n".utf8).write(to: fakeRWS)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: fakeRWS.path)
        let config = tmp.appendingPathComponent("config ' file")
        let command = "claude; echo LOCAL"
        let script = AgentLauncher.script(binary: fakeRWS, config: config, workspace: "demo ' space", executable: command)
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = ["-c", script]
        process.standardOutput = pipe
        try process.run()
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        XCTAssertEqual(process.terminationStatus, 0)
        XCTAssertEqual(String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init),
                       ["--config", config.path, "agent", "--workspace", "demo ' space", "--", command])
    }
}
