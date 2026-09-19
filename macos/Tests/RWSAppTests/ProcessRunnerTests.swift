import XCTest
@testable import RWSApp

final class ProcessRunnerTests: XCTestCase {
    func testProcessScopedOverridePreservesLiteralPath() async throws {
        let path = "/a path/sshfs;literal"
        let result = try await ProcessRunner().run(
            executable: URL(fileURLWithPath: "/bin/sh"),
            arguments: ["-c", "printf '%s' \"$RWS_SSHFS\""],
            environmentOverrides: ["RWS_SSHFS": path])
        XCTAssertEqual(result.exitCode, 0)
        XCTAssertEqual(result.stdout, path)
    }

    func testRunnerDoesNotInvokeAShell() async throws {
        let result = try await ProcessRunner(outputLimit: 1024).run(
            executable: URL(fileURLWithPath: "/usr/bin/printf"),
            arguments: ["%s", "hello; touch /tmp/not-run"]
        )
        XCTAssertEqual(result.stdout, "hello; touch /tmp/not-run")
        XCTAssertEqual(result.exitCode, 0)
    }

    func testFastProcessesRetainExactOutput() async throws {
        let runner = ProcessRunner(outputLimit: 1024)
        for index in 0..<100 {
            let expected = "disconnected\nworkspace: connected (verified RWS mount)\ntail-\(index)"
            let result = try await runner.run(
                executable: URL(fileURLWithPath: "/usr/bin/printf"),
                arguments: ["%s", expected]
            )
            XCTAssertEqual(result.stdout, expected)
        }
    }

    func testRunnerBoundsCapturedOutput() async throws {
        let result = try await ProcessRunner(outputLimit: 16).run(
            executable: URL(fileURLWithPath: "/usr/bin/printf"),
            arguments: ["12345678901234567890"]
        )
        XCTAssertLessThanOrEqual(result.stdout.utf8.count, 16)
        XCTAssertTrue(result.outputWasTruncated)
    }

    func testRunnerReturnsAfterDeadline() async {
        let start = ContinuousClock.now
        do {
            _ = try await ProcessRunner(outputLimit: 1024, timeout: .milliseconds(100)).run(
                executable: URL(fileURLWithPath: "/bin/sleep"),
                arguments: ["10"]
            )
            XCTFail("Expected timeout")
        } catch ProcessRunnerError.timedOut {
            XCTAssertLessThan(start.duration(to: .now), .seconds(2))
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }

    func testRunnerDoesNotWaitForDescendantHoldingOutputPipe() async throws {
        let start = ContinuousClock.now
        _ = try await ProcessRunner(outputLimit: 1024, timeout: .milliseconds(100)).run(
            executable: URL(fileURLWithPath: "/bin/sh"),
            arguments: ["-c", "sleep 5 & exit 0"]
        )
        XCTAssertLessThan(start.duration(to: .now), .seconds(1))
    }

    func testFinalDrainStaysBoundedWhileDescendantWrites() async throws {
        let start = ContinuousClock.now
        let result = try await ProcessRunner(outputLimit: 16, timeout: .seconds(1)).run(
            executable: URL(fileURLWithPath: "/bin/sh"),
            arguments: ["-c", "(while :; do printf 1234567890; done) & sleep 0.05; exit 0"]
        )
        XCTAssertLessThanOrEqual(result.stdout.utf8.count + result.stderr.utf8.count, 16)
        XCTAssertTrue(result.outputWasTruncated)
        XCTAssertLessThan(start.duration(to: .now), .seconds(1))
    }

    func testRunnerKillsImmediateChildThatIgnoresTermination() async {
        let start = ContinuousClock.now
        do {
            _ = try await ProcessRunner(outputLimit: 1024, timeout: .milliseconds(100)).run(
                executable: URL(fileURLWithPath: "/bin/sh"),
                arguments: ["-c", "trap '' TERM; while :; do sleep 1; done"]
            )
            XCTFail("Expected timeout")
        } catch ProcessRunnerError.timedOut {
            XCTAssertLessThan(start.duration(to: .now), .seconds(2))
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
    }
}
