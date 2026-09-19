import AppKit

/// A local terminal hosts SSH; only RWS's remote agent command starts the agent.
enum AgentLauncher {
    private static func quote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func script(binary: URL, config: URL, workspace: String, executable: String) -> String {
        "#!/bin/sh\nexec " + [binary.path, "--config", config.path, "agent", "--workspace", workspace, "--", executable]
            .map(quote).joined(separator: " ") + "\n"
    }

    @MainActor
    static func launch(binary: URL, config: URL, workspace: String, executable: String) async throws {
        let directory = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("RWS/AgentLaunchers", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true,
                                               attributes: [.posixPermissions: 0o700])
        // Unique launcher prevents a second click from changing a pending launch.
        let file = directory.appendingPathComponent("Agent-VM-\(UUID().uuidString).command")
        try Data(script(binary: binary, config: config, workspace: workspace, executable: executable).utf8).write(to: file, options: .atomic)
        try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: file.path)
        guard let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") else {
            throw NSError(domain: "RWS", code: 1, userInfo: [NSLocalizedDescriptionKey: "Terminal est introuvable sur ce Mac."])
        }
        try await NSWorkspace.shared.open([file], withApplicationAt: terminal, configuration: NSWorkspace.OpenConfiguration())
    }
}
