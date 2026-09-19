import Foundation

/// Only inspect known locations and app ancestors; never scan the user's disk.
enum StartupDiscovery {
    static func configurations(defaultURL: URL, remembered: URL?, appURL: URL, home: URL) -> [URL] {
        let fm = FileManager.default
        if fm.fileExists(atPath: defaultURL.path) { return [defaultURL] }
        if let remembered, fm.fileExists(atPath: remembered.path) { return [remembered] }
        var candidates = [home.appendingPathComponent(".config/rws/config.json")]
        var directory = appURL.deletingLastPathComponent()
        for _ in 0..<7 {
            candidates.append(directory.appendingPathComponent(".rws-local/config.json"))
            let parent = directory.deletingLastPathComponent()
            if parent == directory { break }
            directory = parent
        }
        var seen = Set<String>()
        return candidates.filter {
            fm.fileExists(atPath: $0.path) && seen.insert($0.resolvingSymlinksInPath().path).inserted
        }
    }

    static func sshfsCandidates(configURL: URL) -> [URL] {
        let fm = FileManager.default
        let buildRoot = configURL.deletingLastPathComponent().appendingPathComponent("sshfs")
        let builds = (try? fm.contentsOfDirectory(at: buildRoot, includingPropertiesForKeys: nil)) ?? []
        let local = builds.filter { $0.lastPathComponent.hasPrefix("build.") }
            .sorted { $0.path < $1.path }.map { $0.appendingPathComponent("sshfs") }
        return (local + [URL(fileURLWithPath: "/usr/local/bin/sshfs"), URL(fileURLWithPath: "/opt/homebrew/bin/sshfs")])
            .filter { fm.isExecutableFile(atPath: $0.path) }
    }
}

enum MountPrerequisites {
    static func problem(macFUSEPresent: Bool, sshfsVersion: String?, fskit: Bool) -> String? {
        guard macFUSEPresent else {
            return "macFUSE est absent ou incomplet. Installez macFUSE, puis cliquez sur Vérifier à nouveau."
        }
        guard let version = sshfsVersion, version.localizedCaseInsensitiveContains("sshfs") else {
            return "SSHFS est absent ou ne démarre pas. Choisissez la version SSHFS corrigée de RWS dans Configuration avancée."
        }
        if fskit && version.range(of: #"rws-fskit([3-9]|[1-9][0-9]+)([^0-9]|$)"#, options: .regularExpression) == nil {
            return "Cette version de SSHFS ne contient pas les corrections FSKit requises. Choisissez la version SSHFS corrigée de RWS."
        }
        return nil
    }
}
