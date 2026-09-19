import AppKit
import SwiftUI

@main
struct RWSDesktopApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            ContentView(model: model)
                .onAppear { appDelegate.model = model }
        }
        .commands {
            CommandGroup(after: .appInfo) {
                Button("Check for Updates…") { Task { await model.updates.checkForUpdates() } }
                    .disabled(!model.updates.isEnabled || model.isBusy)
            }
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    weak var model: AppModel?

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model, !model.isBusy else {
            model?.alertMessage = "RWS cannot quit or install an update while an operation is running."
            return .terminateCancel
        }
        guard model.updatesPreparing || model.updateInstallPending else { return .terminateNow }
        Task { @MainActor in
            await model.refreshStatusForUpdate()
            let safe = await model.updateGuard.mayInstallUpdate()
            if !safe {
                model.updatesPreparing = false
                model.alertMessage = "RWS cannot quit or install an update while a registered volume is mounted or mount state is unknown. Disconnect volumes and refresh Status first."
            }
            sender.reply(toApplicationShouldTerminate: safe)
        }
        return .terminateLater
    }
}
