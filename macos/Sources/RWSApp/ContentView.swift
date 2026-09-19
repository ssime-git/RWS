import AppKit
import SwiftUI

struct ContentView: View {
    @ObservedObject var model: AppModel
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
                    workspaceControls
                    Divider()
                    addWorkspace
                    Divider()
                    DisclosureGroup("Configuration du Mac", isExpanded: $showSetup) { setup.padding(.top, 8) }
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
        .disabled(model.isBusy || model.updatesPreparing || model.operationStateUncertain)
        .overlay { if model.isBusy { ProgressView().controlSize(.large) } }
        .task {
            model.load()
            showSetup = model.configuration.workspaces.isEmpty
            sshfsPath = model.configuration.mount.sshfs ?? ""
            useFSKit = model.configuration.mount.fskit
            await model.refreshStatus()
        }
        .onChange(of: model.configuration.mount.sshfs) { sshfsPath = $0 ?? "" }
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

    private var workspaceControls: some View {
        HStack {
            Button("Ouvrir dans le Finder") { Task { await model.openSelected() } }
                .buttonStyle(.borderedProminent)
                .disabled(model.selectedWorkspace == nil)
            Button("Déconnecter") { Task { await model.disconnectSelected() } }
                .disabled(model.selectedWorkspace == nil)
            Button("Actualiser") { Task { await model.refreshStatus() } }
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
                .disabled(name.isEmpty || host.isEmpty || !remotePath.hasPrefix("/"))
            }
            .padding(.top, 8)
        }
    }

    private var setup: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 10) {
                Text("RWS nécessite actuellement macFUSE et la version SSHFS corrigée externe. Sélectionnez son exécutable ; il n’est pas inclus dans l’app.")
                    .foregroundStyle(.secondary)
                HStack {
                    TextField("/absolute/path/to/sshfs", text: $sshfsPath)
                    Button("Choisir…") { chooseSSHFS() }
                }
                Toggle("Utiliser le moteur macFUSE FSKit", isOn: $useFSKit)
                HStack {
                    Button("Enregistrer") { Task { await model.saveSettings(sshfs: sshfsPath, fskit: useFSKit) } }
                        .disabled(!sshfsPath.hasPrefix("/"))
                    Button("Importer une configuration…") { importConfig() }
                    Button("Installer les règles Delta…") { showDeltaConfirmation = true }
                    Link("Aide à l’installation", destination: URL(string: "https://github.com/ssime-git/RWS/blob/main/docs/prototype.md")!)
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
