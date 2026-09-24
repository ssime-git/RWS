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
    @State private var useNFS = false
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
            useNFS = model.configuration.mount.nfs
        }
        .onChange(of: model.configuration.mount.sshfs) { sshfsPath = $0 ?? "" }
        .onChange(of: model.detectedSSHFS) { if model.configuration.mount.sshfs == nil { sshfsPath = $0 ?? "" } }
        .onChange(of: model.configuration.mount.fskit) { useFSKit = $0 }
        .onChange(of: model.configuration.mount.nfs) { useNFS = $0 }
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
        GroupBox("Ajouter un espace") {
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 10) {
                GridRow { Text("Nom"); TextField("demo", text: $name) }
                GridRow { Text("Hôte SSH"); TextField("user@example", text: $host) }
                GridRow { Text("Dossier distant"); TextField("/chemin/absolu", text: $remotePath) }
            }
            HStack {
                Spacer()
                Button("Ajouter") {
                    Task { await model.add(name: name, host: host, remotePath: remotePath, mountPath: "/Volumes/RWS-\(name)") }
                }
                .disabled(!model.configurationReady || name.isEmpty || host.isEmpty || !remotePath.hasPrefix("/"))
            }
            .padding(.top, 8)
        }
    }

    private var setup: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text(useNFS ? "NFS natif utilise le client intégré à macOS. Le serveur Linux et le point de montage doivent être préparés avant la première connexion." : "SSHFS utilise macFUSE et la version corrigée. La détection est automatique ; choisissez un autre exécutable uniquement si nécessaire.")
                    .foregroundStyle(.secondary)
                HStack {
                    TextField("/absolute/path/to/sshfs", text: $sshfsPath).disabled(useNFS)
                    Button("Choisir…") { chooseSSHFS() }.disabled(useNFS)
                }
                Toggle("Utiliser NFS natif (expérimental)", isOn: $useNFS)
                Toggle("Utiliser le moteur macFUSE FSKit", isOn: $useFSKit).disabled(useNFS)
                HStack {
                    Button("Enregistrer") { Task { await model.saveSettings(sshfs: sshfsPath, fskit: useFSKit && !useNFS, nfs: useNFS) } }
                        .disabled(!model.configurationReady || (!useNFS && !sshfsPath.hasPrefix("/")))
                    Button("Préparer ce Mac pour NFS") { Task { await model.prepareSelectedNFS() } }
                        .disabled(!model.configuration.mount.nfs || model.selectedWorkspace == nil || !model.configurationReady)
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
