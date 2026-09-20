import AppKit
import Foundation

@MainActor
final class AppModel: ObservableObject {
    @Published var configuration = AppConfiguration(version: 1, workspaces: [], mount: MountOptions())
    @Published var selectedWorkspace: String?
    @Published var isBusy = false
    @Published var output = ""
    @Published var alertMessage: String?
    @Published var updatesPreparing = false
    @Published var updateInstallPending = false
    @Published private(set) var operationStateUncertain = false
    @Published private(set) var safeToTerminate = false

    @Published var startupMessage = "Vérification de la configuration…"
    @Published var prerequisiteProblem: String?
    @Published var configurationCandidates: [URL] = []
    @Published var configurationReady = false
    @Published var checkingStartup = false
    @Published private(set) var prerequisitesReady = false
    @Published private(set) var detectedSSHFS: String?
    private var started = false
    private let preferences = UserDefaults.standard

    let updateGuard = UpdateGuard()
    lazy var updates = UpdateManager(model: self)
    private let runner = ProcessRunner()

    static var defaultConfigurationURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("RWS", isDirectory: true)
            .appendingPathComponent("config.json")
    }

    @Published private(set) var configurationURL = AppModel.defaultConfigurationURL
    var cliURL: URL {
        Bundle.main.resourceURL?.appendingPathComponent("bin/rws") ?? URL(fileURLWithPath: "/missing/rws")
    }

    func load() {
        do {
            let data = try Data(contentsOf: configurationURL)
            configuration = try AppConfiguration.decode(data)
            selectedWorkspace = selectedWorkspace ?? configuration.workspaces.first?.name
        } catch CocoaError.fileReadNoSuchFile {
            configuration = AppConfiguration(version: 1, workspaces: [], mount: MountOptions())
        } catch { alertMessage = error.localizedDescription }
    }

    func startup() async {
        guard !started else { return }
        started = true
        checkingStartup = true
        safeToTerminate = false
        await updateGuard.beginOperation()
        let remembered = preferences.string(forKey: "configurationSource").map { URL(fileURLWithPath: $0) }
        configurationCandidates = StartupDiscovery.configurations(
            defaultURL: Self.defaultConfigurationURL, remembered: remembered,
            appURL: Bundle.main.bundleURL, home: FileManager.default.homeDirectoryForCurrentUser)
        if configurationCandidates.count == 1 {
            await selectConfiguration(configurationCandidates[0])
        } else if configurationCandidates.isEmpty {
            configurationReady = true
            startupMessage = "Ajoutez votre premier espace distant."
            await checkPrerequisites()
        } else {
            startupMessage = "Plusieurs configurations trouvées : choisissez celle à utiliser."
        }
        await updateGuard.endOperation()
        checkingStartup = false
    }

    func importConfiguration(_ source: URL) async {
        checkingStartup = true
        safeToTerminate = false
        await updateGuard.beginOperation()
        await selectConfiguration(source)
        await updateGuard.endOperation()
        checkingStartup = false
    }

    private func selectConfiguration(_ source: URL) async {
        do {
            if configurationReady, !configuration.workspaces.isEmpty,
               source.resolvingSymlinksInPath() != configurationURL.resolvingSymlinksInPath() {
                guard FileManager.default.fileExists(atPath: configurationURL.path) else {
                    throw NSError(domain: "RWSConfiguration", code: 2, userInfo: [NSLocalizedDescriptionKey: "La configuration active a disparu ; vérifiez ses montages avant de changer de configuration."])
                }
                let current = try await runner.run(executable: cliURL, arguments: CLICommand.status(config: configurationURL, workspace: nil).arguments)
                guard current.exitCode == 0, !current.outputWasTruncated,
                      MountStatusClassifier.classify(current.stdout + current.stderr) == .inactive else {
                    throw NSError(domain: "RWSConfiguration", code: 2, userInfo: [NSLocalizedDescriptionKey: "Déconnectez les espaces de la configuration actuelle avant d’en choisir une autre."])
                }
            }
            guard FileManager.default.fileExists(atPath: source.path) else {
                throw CocoaError(.fileReadNoSuchFile)
            }
            // Rust is the schema authority. Decode its normalized JSON, not a
            // second, independent interpretation of the selected source file.
            let result = try await runner.run(executable: cliURL, arguments: CLICommand.list(config: source).arguments)
            guard result.exitCode == 0, !result.outputWasTruncated else {
                throw NSError(domain: "RWSConfiguration", code: 1, userInfo: [NSLocalizedDescriptionKey: result.stderr.isEmpty ? "La configuration n’a pas pu être validée." : result.stderr])
            }
            let decoded = try AppConfiguration.decode(Data(result.stdout.utf8))
            configurationURL = source
            configuration = decoded
            selectedWorkspace = decoded.workspaces.first?.name
            configurationReady = true
            configurationCandidates = []
            preferences.set(source.path, forKey: "configurationSource")
            startupMessage = "Configuration retrouvée — \(decoded.workspaces.count) espace(s)."
            await checkPrerequisites()
            await refreshIntegrations()
            await refreshStatus()
            if let mounted = decoded.workspaces.first(where: { output.contains("\($0.name): connected (verified RWS mount)") }) {
                selectedWorkspace = mounted.name
            }
        } catch {
            startupMessage = "Configuration inutilisable : \(source.path)"
            alertMessage = "Impossible d’utiliser \(source.path) :\n\(error.localizedDescription)"
        }
    }

    func recheckStartup() async {
        guard !checkingStartup, !isBusy else { return }
        checkingStartup = true
        safeToTerminate = false
        await updateGuard.beginOperation()
        if configurationReady {
            await checkPrerequisites()
            await refreshStatus()
        } else if configurationCandidates.count == 1 {
            await selectConfiguration(configurationCandidates[0])
        }
        await updateGuard.endOperation()
        checkingStartup = false
    }

    private func checkPrerequisites() async {
        prerequisitesReady = false
        detectedSSHFS = nil
        let fm = FileManager.default
        let fusePresent = fm.fileExists(atPath: "/Library/Filesystems/macfuse.fs/Contents/Info.plist")
            && fm.fileExists(atPath: "/usr/local/lib/libfuse3.4.dylib")
        guard fusePresent else {
            prerequisiteProblem = MountPrerequisites.problem(macFUSEPresent: false, sshfsVersion: nil, fskit: true)
            return
        }
        let configured = configuration.mount.sshfs.map { URL(fileURLWithPath: $0) }
        let candidates = configured.map { [$0] } ?? StartupDiscovery.sshfsCandidates(configURL: configurationURL)
        var version: String?
        var diagnostic = ""
        for candidate in candidates {
            do {
                let result = try await ProcessRunner(timeout: .seconds(5)).run(executable: candidate, arguments: ["--version"])
                let text = result.stdout + result.stderr
                let needsFSKit = configured == nil ? true : configuration.mount.fskit
                if result.exitCode == 0, !result.outputWasTruncated,
                   MountPrerequisites.problem(macFUSEPresent: true, sshfsVersion: text, fskit: needsFSKit) == nil {
                    version = text
                    detectedSSHFS = candidate.path
                    break
                }
                diagnostic = text
                if configured != nil { version = result.exitCode == 0 && !result.outputWasTruncated ? text : nil }
            } catch { diagnostic = error.localizedDescription }
        }
        prerequisiteProblem = MountPrerequisites.problem(macFUSEPresent: true, sshfsVersion: version, fskit: configuration.mount.fskit)
        prerequisitesReady = prerequisiteProblem == nil
        if !prerequisitesReady, !diagnostic.isEmpty { output = diagnostic }
    }

    func add(name: String, host: String, remotePath: String, mountPath: String) async {
        guard await perform(.add(config: configurationURL, name: name, host: host, remotePath: remotePath, mountPath: mountPath), refreshAfter: false) else { return }
        load()
        selectedWorkspace = name
        await refreshIntegrations()
        guard prerequisitesReady else {
            alertMessage = "Espace enregistré. Configurez macFUSE et SSHFS pour le monter et l’ajouter au Finder."
            await refreshStatus()
            return
        }
        await openSelected()
    }

    /// Keep the zsh auto-shell integration and any previously installed
    /// Delta rules current for the active configuration and bundled CLI.
    /// Failure never blocks the app. Delta rules are only refreshed, never
    /// installed here — first installation stays an explicit user action.
    private func refreshIntegrations() async {
        guard configurationReady else { return }
        if preferences.object(forKey: "installShellHook") as? Bool ?? true {
            await runIntegration(.hookInstall(config: configurationURL),
                                 failure: "Intégration terminal non installée")
        }
        if preferences.object(forKey: "refreshDeltaRules") as? Bool ?? true {
            await runIntegration(.deltaRulesRefresh(config: configurationURL),
                                 failure: "Règles Delta non rafraîchies")
        }
    }

    private func runIntegration(_ command: CLICommand, failure: String) async {
        do {
            let result = try await runner.run(executable: cliURL, arguments: command.arguments)
            if result.exitCode != 0 {
                output = "\(failure) : \(result.stderr.isEmpty ? result.stdout : result.stderr)"
            }
        } catch {
            output = "\(failure) : \(error.localizedDescription)"
        }
    }

    func saveSettings(sshfs: String, fskit: Bool) async {
        await perform(.settings(config: configurationURL, sshfs: sshfs, fskit: fskit))
        load()
        await recheckStartup()
    }

    func openSelected() async {
        guard configurationReady, prerequisitesReady, let selectedWorkspace,
              let workspace = configuration.workspaces.first(where: { $0.name == selectedWorkspace }) else { return }
        let succeeded = await perform(.connect(config: configurationURL, workspace: selectedWorkspace), refreshAfter: false)
        if succeeded {
            let folder = URL(fileURLWithPath: workspace.mountRoot, isDirectory: true)
            do { try FinderSidebar.pin(folder) }
            catch { alertMessage = "Le dossier est monté, mais son ajout à la barre latérale a échoué : \(error.localizedDescription) Dans le Finder, utilisez Fichier > Ajouter à la barre latérale." }
            NSWorkspace.shared.open(folder)
        }
        await refreshStatus()
    }

    func disconnectSelected() async {
        guard let selectedWorkspace,
              let workspace = configuration.workspaces.first(where: { $0.name == selectedWorkspace }) else { return }
        let folder = URL(fileURLWithPath: workspace.mountRoot, isDirectory: true)
        let legacyName = folder.path.hasPrefix("/Volumes/RWS-")
            ? (try? folder.resourceValues(forKeys: [.volumeNameKey]))?.volumeName : nil
        if await perform(.disconnect(config: configurationURL, workspace: selectedWorkspace), refreshAfter: false) {
            do { try FinderSidebar.remove(folder, legacyVolumeName: legacyName) }
            catch { alertMessage = "Dossier déconnecté, mais le raccourci Finder n’a pas pu être retiré : \(error.localizedDescription)" }
        }
        await refreshStatus()
    }

    func launchAgent(_ executable: String, on target: AgentTarget = .vm) async {
        let executable = executable.trimmingCharacters(in: .whitespacesAndNewlines)
        guard configurationReady, !isBusy, !updatesPreparing, !executable.isEmpty,
              !executable.hasPrefix("-"), !executable.contains("\0"), let selectedWorkspace else { return }
        let mode: AgentLauncher.Mode
        switch target {
        case .vm:
            mode = .remote
        case .localMount:
            guard let workspace = configuration.workspaces.first(where: { $0.name == selectedWorkspace }) else { return }
            mode = .local(mountRoot: workspace.mountRoot)
        }
        isBusy = true
        await updateGuard.beginOperation()
        do {
            try await AgentLauncher.launch(binary: cliURL, config: configurationURL,
                                           workspace: selectedWorkspace, executable: executable, mode: mode)
            output = switch target {
            case .vm:
                "Terminal distant ouvert pour \(selectedWorkspace). Le terminal affichera le résultat de la connexion et l’identité de la VM."
            case .localMount:
                "Terminal local ouvert dans le dossier monté de \(selectedWorkspace). Les commandes s’exécutent sur ce Mac."
            }
        } catch { alertMessage = error.localizedDescription }
        await updateGuard.endOperation()
        isBusy = false
    }

    enum AgentTarget { case vm, localMount }

    func installDeltaRules() async { await perform(.deltaRules(config: configurationURL)) }

    func refreshStatus() async {
        guard configurationReady, !isBusy else { return }
        let succeeded = await perform(.status(config: configurationURL, workspace: nil), refreshAfter: false)
        if succeeded {
            let state = MountStatusClassifier.classify(output)
            await updateGuard.setMountState(state)
            safeToTerminate = state == .inactive
        } else {
            await updateGuard.setMountState(.unknown)
            safeToTerminate = false
        }
    }

    func refreshStatusForUpdate() async {
        guard !isBusy, !checkingStartup else { return }
        let succeeded = await perform(.status(config: configurationURL, workspace: nil), refreshAfter: false, allowDuringUpdatePreparation: true)
        let state = succeeded ? MountStatusClassifier.classify(output) : .unknown
        await updateGuard.setMountState(state)
        safeToTerminate = state == .inactive
    }

    @discardableResult
    private func perform(_ command: CLICommand, refreshAfter: Bool = true, allowDuringUpdatePreparation: Bool = false) async -> Bool {
        guard !operationStateUncertain else {
            alertMessage = "A previous RWS process exceeded its deadline. Restart RWS before starting another operation."
            return false
        }
        guard !isBusy, allowDuringUpdatePreparation || !updatesPreparing else { return false }
        isBusy = true
        safeToTerminate = false
        await updateGuard.beginOperation()
        do {
            let result = try await runner.run(executable: cliURL, arguments: command.arguments, environmentOverrides: detectedSSHFS.map { ["RWS_SSHFS": $0] } ?? [:])
            output = [result.stdout, result.stderr].filter { !$0.isEmpty }.joined(separator: "\n")
            if result.outputWasTruncated { output += "\n[Output truncated]" }
            guard result.exitCode == 0 else {
                alertMessage = output.isEmpty ? "RWS exited with status \(result.exitCode)." : output
                await updateGuard.setMountState(.unknown)
                await updateGuard.endOperation()
                isBusy = false
                return false
            }
            await updateGuard.endOperation()
            isBusy = false
            if refreshAfter { Task { await self.refreshStatus() } }
            return true
        } catch {
            alertMessage = error.localizedDescription
            if case ProcessRunnerError.timedOut = error { operationStateUncertain = true }
            await updateGuard.setMountState(.unknown)
            await updateGuard.endOperation()
            isBusy = false
            return false
        }
    }
}
