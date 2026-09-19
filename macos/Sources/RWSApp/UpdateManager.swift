import Foundation
import Sparkle

@MainActor
final class UpdateManager: NSObject, ObservableObject, SPUUpdaterDelegate {
    private weak var model: AppModel?
    private var controller: SPUStandardUpdaterController?
    private var postponedInstallHandler: (() -> Void)?
    let isEnabled: Bool

    init(model: AppModel, bundle: Bundle = .main) {
        self.model = model
        let info = bundle.infoDictionary ?? [:]
        let key = (info["SUPublicEDKey"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let feed = (info["SUFeedURL"] as? String).flatMap(URL.init(string:))
        isEnabled = info["RWSUpdatesEnabled"] as? Bool == true
            && !key.isEmpty && !key.contains("PLACEHOLDER")
            && feed?.scheme?.lowercased() == "https"
        super.init()
        if isEnabled {
            let controller = SPUStandardUpdaterController(startingUpdater: false, updaterDelegate: self, userDriverDelegate: nil)
            controller.updater.automaticallyDownloadsUpdates = false
            controller.startUpdater()
            self.controller = controller
        }
    }

    func checkForUpdates() async {
        guard let model, !model.checkingStartup, await model.updateGuard.mayInstallUpdate() else {
            model?.alertMessage = "Updates are available only after every RWS operation finishes and all registered volumes are disconnected. Refresh Status and try again."
            return
        }
        if let installHandler = postponedInstallHandler {
            postponedInstallHandler = nil
            model.updatesPreparing = true
            model.updateInstallPending = true
            installHandler()
            return
        }
        controller?.checkForUpdates(nil)
    }

    func updater(_ updater: SPUUpdater, mayPerform updateCheck: SPUUpdateCheck, error: AutoreleasingUnsafeMutablePointer<NSError?>) -> Bool {
        guard model?.safeToTerminate == true, model?.checkingStartup == false else {
            error.pointee = NSError(
                domain: "RWSUpdateGuard", code: 1,
                userInfo: [NSLocalizedDescriptionKey: "Disconnect every registered RWS volume before checking for updates."]
            )
            return false
        }
        return true
    }

    func updater(_ updater: SPUUpdater, shouldPostponeRelaunchForUpdate update: SUAppcastItem, untilInvokingBlock installHandler: @escaping () -> Void) -> Bool {
        guard let model else { return true }
        model.updatesPreparing = true
        postponedInstallHandler = installHandler
        Task { @MainActor in
            await model.refreshStatusForUpdate()
            if await model.updateGuard.mayInstallUpdate() {
                self.postponedInstallHandler = nil
                model.updateInstallPending = true
                installHandler()
            } else {
                model.updatesPreparing = false
                model.alertMessage = "The update was postponed because an RWS operation or registered mount is active, or mount state could not be verified."
            }
        }
        return true
    }
}
