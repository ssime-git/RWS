import Foundation
import Darwin

struct ProcessResult: Sendable {
    let exitCode: Int32
    let stdout: String
    let stderr: String
    let outputWasTruncated: Bool
}

enum ProcessRunnerError: LocalizedError {
    case executableMissing(String)
    case launch(String)
    case timedOut

    var errorDescription: String? {
        switch self {
        case .executableMissing(let path): "The bundled RWS command was not found at \(path). Rebuild the app bundle."
        case .launch(let message): "RWS could not start: \(message)"
        case .timedOut: "RWS did not finish within the allowed time. The operation was stopped; refresh Status before continuing."
        }
    }
}

final class ProcessRunner: @unchecked Sendable {
    private let outputLimit: Int
    private let timeout: Duration

    init(outputLimit: Int = 256 * 1024, timeout: Duration = .seconds(45)) {
        self.outputLimit = max(1, outputLimit)
        self.timeout = timeout
    }

    func run(executable: URL, arguments: [String], environmentOverrides: [String: String] = [:]) async throws -> ProcessResult {
        guard FileManager.default.isExecutableFile(atPath: executable.path) else {
            throw ProcessRunnerError.executableMissing(executable.path)
        }
        return try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            let stdoutPipe = Pipe()
            let stderrPipe = Pipe()
            let capture = BoundedCapture(limit: outputLimit)
            let completion = ProcessCompletion(continuation)
            let ioQueue = DispatchQueue(label: "io.github.ssime-git.RWS.process-output")
            let drainLimit = outputLimit + 1
            let timeoutSeconds = Double(timeout.components.seconds) + Double(timeout.components.attoseconds) / 1e18
            let timeoutWork = DispatchWorkItem {
                guard completion.requestTimeout() else { return }
                stdoutPipe.fileHandleForReading.readabilityHandler = nil
                stderrPipe.fileHandleForReading.readabilityHandler = nil
                ioQueue.sync {
                    try? stdoutPipe.fileHandleForReading.close()
                    try? stderrPipe.fileHandleForReading.close()
                }
                if process.isRunning {
                    _ = Darwin.kill(process.processIdentifier, SIGKILL)
                    DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 0.5) {
                        _ = completion.finish(.failure(ProcessRunnerError.timedOut))
                    }
                } else {
                    _ = completion.finish(.failure(ProcessRunnerError.timedOut))
                }
            }
            process.executableURL = executable
            process.arguments = arguments
            process.environment = ProcessInfo.processInfo.environment.merging(environmentOverrides) { _, selected in selected }
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = stdoutPipe
            process.standardError = stderrPipe
            stdoutPipe.fileHandleForReading.readabilityHandler = { handle in
                ioQueue.sync { capture.appendStdout(handle.availableData) }
            }
            stderrPipe.fileHandleForReading.readabilityHandler = { handle in
                ioQueue.sync { capture.appendStderr(handle.availableData) }
            }
            process.terminationHandler = { process in
                if completion.isFinished { return }
                stdoutPipe.fileHandleForReading.readabilityHandler = nil
                stderrPipe.fileHandleForReading.readabilityHandler = nil
                if completion.timeoutWasRequested {
                    _ = completion.finish(.failure(ProcessRunnerError.timedOut))
                    return
                }
                ioQueue.sync {
                    capture.appendStdout(Self.drainAvailable(stdoutPipe.fileHandleForReading, limit: drainLimit))
                    capture.appendStderr(Self.drainAvailable(stderrPipe.fileHandleForReading, limit: drainLimit))
                    try? stdoutPipe.fileHandleForReading.close()
                    try? stderrPipe.fileHandleForReading.close()
                }
                if completion.finish(.success(capture.result(exitCode: process.terminationStatus))) {
                    timeoutWork.cancel()
                }
            }
            do {
                try process.run()
                DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeoutSeconds, execute: timeoutWork)
            } catch {
                _ = completion.finish(.failure(ProcessRunnerError.launch(error.localizedDescription)))
            }
        }
    }

    private static func drainAvailable(_ handle: FileHandle, limit: Int) -> Data {
        let descriptor = handle.fileDescriptor
        let flags = fcntl(descriptor, F_GETFL)
        guard flags >= 0, fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) >= 0 else { return Data() }
        var result = Data()
        var buffer = [UInt8](repeating: 0, count: 8 * 1024)
        while result.count < limit {
            let requested = min(buffer.count, limit - result.count)
            let count = buffer.withUnsafeMutableBytes { bytes in
                Darwin.read(descriptor, bytes.baseAddress, requested)
            }
            guard count > 0 else { break }
            result.append(buffer, count: count)
        }
        return result
    }
}

private final class ProcessCompletion: @unchecked Sendable {
    private let lock = NSLock()
    private var finished = false
    private var timedOut = false
    private let continuation: CheckedContinuation<ProcessResult, Error>

    init(_ continuation: CheckedContinuation<ProcessResult, Error>) { self.continuation = continuation }

    var isFinished: Bool {
        lock.lock(); defer { lock.unlock() }
        return finished
    }

    var timeoutWasRequested: Bool {
        lock.lock(); defer { lock.unlock() }
        return timedOut
    }

    func requestTimeout() -> Bool {
        lock.lock(); defer { lock.unlock() }
        guard !finished else { return false }
        timedOut = true
        return true
    }

    @discardableResult
    func finish(_ result: Result<ProcessResult, Error>) -> Bool {
        lock.lock()
        guard !finished else { lock.unlock(); return false }
        finished = true
        lock.unlock()
        continuation.resume(with: result)
        return true
    }
}

private final class BoundedCapture: @unchecked Sendable {
    private let lock = NSLock()
    private let limit: Int
    private var stdout = Data()
    private var stderr = Data()
    private var truncated = false

    init(limit: Int) { self.limit = limit }

    func appendStdout(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        let remaining = max(0, limit - stdout.count - stderr.count)
        if data.count > remaining { truncated = true }
        stdout.append(data.prefix(remaining))
    }

    func appendStderr(_ data: Data) {
        lock.lock(); defer { lock.unlock() }
        let remaining = max(0, limit - stdout.count - stderr.count)
        if data.count > remaining { truncated = true }
        stderr.append(data.prefix(remaining))
    }

    func result(exitCode: Int32) -> ProcessResult {
        lock.lock(); defer { lock.unlock() }
        return ProcessResult(
            exitCode: exitCode,
            stdout: String(decoding: stdout, as: UTF8.self),
            stderr: String(decoding: stderr, as: UTF8.self),
            outputWasTruncated: truncated
        )
    }
}
