import AppKit
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: AppModel
    @State private var agentExecutable = "claude"
    @State private var agentSubdirectory = ""
    @State private var name = ""
    @State private var host = ""
    @State private var remotePath = ""
    @State private var sshfsPath = ""
    @State private var useFSKit = true
    @State private var showDeltaConfirmation = false
    @State private var showSetup = false
    @State private var showDetails = false

    var body: some View {
        NavigationSplitView {
            List(model.configuration.workspaces, selection: $model.selectedWorkspace) { workspace in
                Text(workspace.name).tag(workspace.name)
            }
            .navigationTitle("Espaces")
        } detail: {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    startupStatus
                    workspaceControls
                    HStack {
                        TextField("Agent : claude, codex…", text: $agentExecutable)
                            .textFieldStyle(.roundedBorder).frame(maxWidth: 200)
                        TextField("Sous-dossier (optionnel)", text: $agentSubdirectory)
                            .textFieldStyle(.roundedBorder).frame(maxWidth: 180)
                        Button("Lancer sur la VM") { Task { await model.launchAgent(agentExecutable, on: .vm, subdirectory: agentSubdirectory) } }
                            .disabled(model.selectedWorkspace == nil || !model.configurationReady || agentExecutable.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        Button("Lancer en local") { Task { await model.launchAgent(agentExecutable, on: .localMount, subdirectory: agentSubdirectory) } }
                            .disabled(model.selectedWorkspace == nil || !model.configurationReady || agentExecutable.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                    Text("Sur la VM : l’agent distant s’exécute dans le dossier distant (sous-dossier relatif de l’espace si renseigné). En local : l’agent de ce Mac s’exécute dans le dossier monté, sans bascule automatique.")
                        .font(.caption).foregroundStyle(.secondary)
                    Divider()
                    addWorkspace
                    Divider()
                    DisclosureGroup("Configuration avancée", isExpanded: $showSetup) { setup.padding(.top, 8) }
                    DisclosureGroup("Détails", isExpanded: $showDetails) {
                        Text(model.output.isEmpty ? "L’état et les détails des commandes apparaissent ici." : model.output)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.top, 8)
                    }
                }
                .padding(24)
            }
            .navigationTitle(model.selectedWorkspace ?? "RWS")
        }
        .frame(minWidth: 820, minHeight: 600)
        .disabled(model.isBusy || model.checkingStartup || model.updatesPreparing || model.operationStateUncertain)
        .overlay { if model.isBusy || model.checkingStartup { ProgressView().controlSize(.large) } }
        .task {
            await model.startup()
            showSetup = false
            sshfsPath = model.configuration.mount.sshfs ?? model.detectedSSHFS ?? ""
            useFSKit = model.configuration.mount.fskit
        }
        .onChange(of: model.configuration.mount.sshfs) { sshfsPath = $0 ?? "" }
        .onChange(of: model.detectedSSHFS) { if model.configuration.mount.sshfs == nil { sshfsPath = $0 ?? "" } }
        .onChange(of: model.configuration.mount.fskit) { useFSKit = $0 }
        .alert("RWS", isPresented: Binding(get: { model.alertMessage != nil }, set: { if !$0 { model.alertMessage = nil } })) {
            Button("OK") { model.alertMessage = nil }
        } message: { Text(model.alertMessage ?? "") }
        .confirmationDialog("Installer les règles Delta ?", isPresented: $showDeltaConfirmation) {
            Button("Installer") { Task { await model.installDeltaRules() } }
        } message: {
            Text("Cette action modifie les instructions personnelles de Delta afin de transférer explicitement les commandes des agents vers l’hôte distant. Delta et les autres processus natifs continuent de s’exécuter sur ce Mac.")
        }
    }

    private var startupStatus: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("RWS \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "dev") · build \(Bundle.main.object(forInfoDictionaryKey: "RWSBuildRevision") as? String ?? "inconnu")")
                .font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
            if let built = Bundle.main.object(forInfoDictionaryKey: "RWSBuildTimestamp") as? String {
                Text("Construit le \(built)").font(.caption2).foregroundStyle(.secondary)
            }
            Text(model.startupMessage).foregroundStyle(.secondary)
            ForEach(model.configurationCandidates, id: \.path) { source in
                Button(source.path) { Task { await model.importConfiguration(source) } }
            }
            if let problem = model.prerequisiteProblem {
                Label(problem, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.orange)
                HStack {
                    Link("Installer macFUSE", destination: URL(string: "https://macfuse.github.io/")!)
                    Button("Vérifier à nouveau") { Task { await model.recheckStartup() } }
                    Button("Configuration avancée") { showSetup = true }
                }
            } else if model.prerequisitesReady {
                Label("macFUSE et SSHFS détectés", systemImage: "checkmark.circle")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var workspaceControls: some View {
        HStack {
            Button("Ouvrir dans le Finder") { Task { await model.openSelected() } }
                .buttonStyle(.borderedProminent)
                .disabled(model.selectedWorkspace == nil || !model.prerequisitesReady || !model.configurationReady)
            Button("Déconnecter") { Task { await model.disconnectSelected() } }
                .disabled(model.selectedWorkspace == nil || !model.configurationReady)
            Button("Réparer") { Task { await model.repairSelected() } }
                .disabled(model.selectedWorkspace == nil || !model.prerequisitesReady || !model.configurationReady)
                .help("Éjecte un montage qui ne répond plus (après une coupure réseau) et le remonte. Refusé si le montage est sain.")
            Button("Actualiser") { Task { await model.recheckStartup() } }
            Spacer()
            if model.updates.isEnabled {
                Button("Rechercher les mises à jour…") { Task { await model.updates.checkForUpdates() } }
            }
        }
    }

    private var addWorkspace: some View {
        GroupBox("Ajouter une VM") {
            VStack(alignment: .leading, spacing: 10) {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                    GridRow { Text("Nom"); TextField("demo", text: $name) }
                    GridRow {
                        Text("Machine")
                        HStack {
                            TextField("utilisateur@machine", text: $host)
                            Menu("Machines Tailscale") {
                                ForEach(model.vmMachines) { machine in
                                    Button("\(machine.name) — \(machine.os)\(machine.online ? "" : " (hors ligne)")") {
                                        host = hostValue(for: machine)
                                        if name.isEmpty { name = machine.name }
                                    }
                                }
                            }
                            .disabled(model.vmMachines.isEmpty)
                            .fixedSize()
                        }
                    }
                    GridRow { Text("Dossier distant"); TextField("/chemin/absolu", text: $remotePath) }
                }
                if !model.vmChecks.isEmpty { vmCheckList }
                if model.vmCheckRunning { ProgressView("Vérification de la VM…").controlSize(.small) }
                HStack {
                    Button("Vérifier") { Task { await model.checkVM(host: host, remote: remotePath) } }
                        .disabled(host.isEmpty || model.vmCheckRunning)
                    Spacer()
                    Button("Ajouter et monter") {
                        Task { await model.addVM(name: name, host: host, remotePath: remotePath) }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(!model.configurationReady || name.isEmpty || host.isEmpty || !remotePath.hasPrefix("/") || model.vmCheckRunning)
                }
                Text("RWS vérifie d’abord le Mac et la VM (Tailscale, SSH, SFTP, dossier) et vous dit exactement quoi corriger. Rien n’est installé à votre place ; le dossier apparaît ensuite dans ~/RWS.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .padding(.top, 4)
        }
        .task { await model.loadMachines() }
    }

    private var vmCheckList: some View {
        VStack(alignment: .leading, spacing: 8) {
            ForEach(model.vmChecks) { item in
                VStack(alignment: .leading, spacing: 2) {
                    Label(OnboardText.detail(item, host: host), systemImage: icon(for: item))
                        .foregroundStyle(item.isFailure ? Color.red : (item.isWarning ? Color.orange : Color.primary))
                    if let fix = OnboardText.fix(item, host: host, remote: remotePath) {
                        Text(fix).font(.caption).foregroundStyle(.secondary)
                            .textSelection(.enabled).padding(.leading, 22)
                    }
                }
            }
        }
    }

    private func icon(for item: OnboardCheckItem) -> String {
        item.isFailure ? "xmark.octagon.fill" : (item.isWarning ? "exclamationmark.triangle.fill" : "checkmark.circle")
    }

    /// Keeps a typed user (`user@…`) and swaps in the chosen machine's name.
    private func hostValue(for machine: OnboardMachine) -> String {
        let user = host.contains("@") ? String(host.split(separator: "@", omittingEmptySubsequences: false)[0]) : ""
        return user.isEmpty ? machine.name : "\(user)@\(machine.name)"
    }

    private var setup: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("RWS nécessite actuellement macFUSE et la version SSHFS corrigée externe. La détection est automatique ; choisissez un autre exécutable uniquement si nécessaire.")
                    .foregroundStyle(.secondary)
                HStack {
                    TextField("/absolute/path/to/sshfs", text: $sshfsPath)
                    Button("Choisir…") { chooseSSHFS() }
                }
                Toggle("Utiliser le moteur macFUSE FSKit", isOn: $useFSKit)
                HStack {
                    Button("Enregistrer") { Task { await model.saveSettings(sshfs: sshfsPath, fskit: useFSKit) } }
                        .disabled(!model.configurationReady || !sshfsPath.hasPrefix("/"))
                    Button("Choisir une configuration…") { importConfig() }
                    Button("Installer les règles Delta…") { showDeltaConfirmation = true }.disabled(!model.configurationReady)
                    Link("Aide à l’installation", destination: URL(string: "https://github.com/ssime-git/RWS/blob/prototype/cli/docs/prototype.md")!)
                }
            }
        }
    }

    private func chooseSSHFS() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { sshfsPath = url.path }
    }

    private func importConfig() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.canChooseFiles = true
        panel.allowsMultipleSelection = false
        if panel.runModal() == .OK, let url = panel.url { Task { await model.importConfiguration(url) } }
    }
}
