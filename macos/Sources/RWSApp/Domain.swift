import Foundation

struct Workspace: Codable, Identifiable, Hashable {
    let name: String
    let host: String
    let remoteRoot: String
    let mountRoot: String

    var id: String { name }

    enum CodingKeys: String, CodingKey {
        case name, host
        case remoteRoot = "remote_root"
        case mountRoot = "mount_root"
    }
}

struct MountOptions: Codable {
    var sshfs: String?
    var fskit: Bool = false

    init(sshfs: String? = nil, fskit: Bool = false) {
        self.sshfs = sshfs
        self.fskit = fskit
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        sshfs = try values.decodeIfPresent(String.self, forKey: .sshfs)
        fskit = try values.decodeIfPresent(Bool.self, forKey: .fskit) ?? false
    }
}

struct AppConfiguration: Codable {
    let version: Int
    let workspaces: [Workspace]
    var mount: MountOptions

    init(version: Int, workspaces: [Workspace], mount: MountOptions = MountOptions()) {
        self.version = version
        self.workspaces = workspaces
        self.mount = mount
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        version = try values.decode(Int.self, forKey: .version)
        workspaces = try values.decode([Workspace].self, forKey: .workspaces)
        mount = try values.decodeIfPresent(MountOptions.self, forKey: .mount) ?? MountOptions()
    }

    static func decode(_ data: Data) throws -> AppConfiguration {
        let object = try JSONSerialization.jsonObject(with: data)
        guard let dictionary = object as? [String: Any],
              Set(dictionary.keys).isSubset(of: ["version", "workspaces", "mount"]),
              let workspaces = dictionary["workspaces"] as? [[String: Any]],
              workspaces.allSatisfy({ Set($0.keys).isSubset(of: ["name", "host", "remote_root", "mount_root"]) }),
              dictionary["mount"].map({ ($0 as? [String: Any]).map { Set($0.keys).isSubset(of: ["sshfs", "fskit"]) } ?? false }) ?? true
        else { throw ConfigurationError.invalidShape }
        let decoded = try JSONDecoder().decode(Self.self, from: data)
        guard decoded.version == 1 else { throw ConfigurationError.unsupportedVersion }
        return decoded
    }
}

enum ConfigurationError: LocalizedError {
    case invalidShape
    case unsupportedVersion
    case destinationExists

    var errorDescription: String? {
        switch self {
        case .invalidShape: "The selected file is not an RWS configuration."
        case .unsupportedVersion: "This RWS configuration version is not supported."
        case .destinationExists: "A configuration already exists. Import will not replace it."
        }
    }
}

enum ConfigurationImporter {
    static func importConfig(from source: URL, to destination: URL) throws {
        _ = try AppConfiguration.decode(Data(contentsOf: source))
        guard !FileManager.default.fileExists(atPath: destination.path) else {
            throw ConfigurationError.destinationExists
        }
        try FileManager.default.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        try FileManager.default.copyItem(at: source, to: destination)
    }
}

struct CLICommand: Equatable {
    let arguments: [String]

    private static func prefix(_ config: URL) -> [String] { ["--config", config.path] }

    static func add(config: URL, name: String, host: String, remotePath: String, mountPath: String) -> Self {
        Self(arguments: prefix(config) + ["workspace", "add", name, "--ssh", host, "--remote", remotePath, "--mount", mountPath])
    }

    static func connect(config: URL, workspace: String) -> Self {
        Self(arguments: prefix(config) + ["connect", workspace])
    }

    static func disconnect(config: URL, workspace: String) -> Self {
        Self(arguments: prefix(config) + ["disconnect", workspace])
    }

    static func status(config: URL, workspace: String?) -> Self {
        Self(arguments: prefix(config) + ["status"] + (workspace.map { [$0] } ?? []) + ["--no-probe"])
    }

    static func settings(config: URL, sshfs: String, fskit: Bool) -> Self {
        Self(arguments: prefix(config) + ["settings", "--sshfs", sshfs, "--backend", fskit ? "fskit" : "default"])
    }

    static func deltaRules(config: URL) -> Self {
        Self(arguments: prefix(config) + ["delta-rules"])
    }
}

enum RegisteredMountState: Sendable, Equatable { case unknown, active, inactive }

enum MountStatusClassifier {
    static func classify(_ output: String) -> RegisteredMountState {
        if output.contains("[Output truncated]") { return .unknown }
        if output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return .inactive }
        let stateLines = output.split(separator: "\n").map(String.init).filter { !$0.hasPrefix(" ") && $0.contains(": ") }
        guard !stateLines.isEmpty else { return .unknown }
        if stateLines.contains(where: { $0.contains(": unavailable:") }) { return .unknown }
        if stateLines.contains(where: { $0.contains(": connected (") || $0.contains(": mounted (") }) { return .active }
        return stateLines.allSatisfy { $0.hasSuffix(": disconnected") } ? .inactive : .unknown
    }
}

actor UpdateGuard {
    private var operationCount = 0
    private var mountState: RegisteredMountState = .unknown

    func beginOperation() { operationCount += 1 }
    func endOperation() { operationCount = max(0, operationCount - 1) }
    func setMountState(_ state: RegisteredMountState) { mountState = state }
    func mayInstallUpdate() -> Bool { operationCount == 0 && mountState == .inactive }
}
