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

    let updateGuard = UpdateGuard()
    lazy var updates = UpdateManager(model: self)
    private let runner = ProcessRunner()

    static var defaultConfigurationURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("RWS", isDirectory: true)
            .appendingPathComponent("config.json")
    }

    var configurationURL = AppModel.defaultConfigurationURL
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

    func importConfiguration(_ source: URL) async {
        do {
            guard !FileManager.default.fileExists(atPath: configurationURL.path) else {
                throw ConfigurationError.destinationExists
            }
            guard await perform(.status(config: source, workspace: nil), refreshAfter: false) else { return }
            try ConfigurationImporter.importConfig(from: source, to: configurationURL)
            load()
            output = "Imported \(source.lastPathComponent). Mount receipts and other files were not copied."
            await refreshStatus()
        } catch { alertMessage = error.localizedDescription }
    }

    func add(name: String, host: String, remotePath: String, mountPath: String) async {
        await perform(.add(config: configurationURL, name: name, host: host, remotePath: remotePath, mountPath: mountPath))
        load()
    }

    func saveSettings(sshfs: String, fskit: Bool) async {
        await perform(.settings(config: configurationURL, sshfs: sshfs, fskit: fskit))
        load()
    }

    func openSelected() async {
        guard let selectedWorkspace,
              let workspace = configuration.workspaces.first(where: { $0.name == selectedWorkspace }) else { return }
        let succeeded = await perform(.connect(config: configurationURL, workspace: selectedWorkspace))
        if succeeded { NSWorkspace.shared.open(URL(fileURLWithPath: workspace.mountRoot, isDirectory: true)) }
    }

    func disconnectSelected() async {
        guard let selectedWorkspace else { return }
        await perform(.disconnect(config: configurationURL, workspace: selectedWorkspace))
    }

    func installDeltaRules() async { await perform(.deltaRules(config: configurationURL)) }

    func refreshStatus() async {
        guard !isBusy else { return }
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
        guard !isBusy else { return }
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
            let result = try await runner.run(executable: cliURL, arguments: command.arguments)
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
