import AppKit

/// A local terminal hosts SSH; only RWS's remote agent command starts the agent.
enum AgentLauncher {
    /// Where the launched agent executes. RWS_AUTO_MODE is exported either
    /// way so shells the agent spawns never prompt for a switch themselves.
    enum Mode: Equatable {
        case remote
        case local(mountRoot: String)
    }

    private static func quote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func script(binary: URL, config: URL, workspace: String, executable: String, mode: Mode) -> String {
        switch mode {
        case .remote:
            return "#!/bin/sh\nRWS_AUTO_MODE=remote\nexport RWS_AUTO_MODE\nexec "
                + [binary.path, "--config", config.path, "agent", "--workspace", workspace, "--", executable]
                    .map(quote).joined(separator: " ") + "\n"
        case .local(let mountRoot):
            return "#!/bin/sh\nRWS_AUTO_MODE=local\nexport RWS_AUTO_MODE\ncd "
                + quote(mountRoot)
                + " || { echo 'RWS: dossier monté introuvable — connectez l’espace d’abord.' >&2; exit 1; }\nexec "
                + quote(executable) + "\n"
        }
    }

    @MainActor
    static func launch(binary: URL, config: URL, workspace: String, executable: String, mode: Mode) async throws {
        let directory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("RWS/AgentLaunchers", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        // Unique launcher prevents a second click from changing a pending launch.
        let kind = if case .remote = mode { "VM" } else { "local" }
        let file = directory.appendingPathComponent("Agent-\(kind)-\(UUID().uuidString).command")
        try Data(script(binary: binary, config: config, workspace: workspace, executable: executable, mode: mode).utf8).write(to: file, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path)
        guard let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") else {
            throw NSError(domain: "RWS", code: 1, userInfo: [NSLocalizedDescriptionKey: "Terminal est introuvable sur ce Mac."])
        }
        try await NSWorkspace.shared.open([file], withApplicationAt: terminal, configuration: NSWorkspace.OpenConfiguration())
    }
}
