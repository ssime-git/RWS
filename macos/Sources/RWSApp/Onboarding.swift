import Foundation

/// One finding of `rws onboard check`. `code` is the CLI's stable reason, used
/// to show French text; `detail` and `fix` are the CLI's own English fallback.
struct OnboardCheckItem: Codable, Equatable, Identifiable {
    let id: String
    let status: String
    let code: String
    let detail: String
    let fix: String?

    var isFailure: Bool { status == "fail" }
    var isWarning: Bool { status == "warn" }
}

struct OnboardMachine: Codable, Equatable, Identifiable {
    let name: String
    let dnsName: String
    let online: Bool
    let os: String
    let ips: [String]

    var id: String { dnsName }
}

struct OnboardReport: Codable, Equatable {
    let ok: Bool
    let checks: [OnboardCheckItem]
    let machines: [OnboardMachine]

    static func decode(_ data: Data) throws -> OnboardReport {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(OnboardReport.self, from: data)
    }
}

struct OnboardStepResult: Codable, Equatable {
    let id: String
    let summary: String
    let status: String
    let output: String
}

struct OnboardAddReport: Codable, Equatable {
    let ok: Bool
    let dryRun: Bool
    let checks: [OnboardCheckItem]
    let results: [OnboardStepResult]
    let mount: String

    static func decode(_ data: Data) throws -> OnboardAddReport {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(OnboardAddReport.self, from: data)
    }
}

/// French wording for the CLI's findings, keyed by their stable codes. An
/// unknown code keeps the CLI's English text rather than hiding information.
enum OnboardText {
    private static let okLabels = [
        "mac.ssh": "SSH disponible",
        "mac.tailscale": "Tailscale est connecté",
        "vm.reachable": "Connexion SSH par clé fonctionnelle",
        "vm.sftp": "La VM répond au protocole SFTP",
        "vm.atomic_save": "Enregistrement atomique supporté",
        "vm.directory": "Dossier distant accessible en écriture",
        "vm.tailscale_peer": "Machine de votre réseau Tailscale",
    ]

    private static let details = [
        "ssh_missing": "La commande ssh est introuvable sur ce Mac.",
        "tailscale_cli_missing": "L’outil en ligne de commande de Tailscale est introuvable : vos machines ne peuvent pas être listées.",
        "tailscale_stopped": "Tailscale est installé mais désactivé.",
        "tailscale_login": "Tailscale demande de vous connecter.",
        "unresolved": "Le nom de la machine est introuvable.",
        "refused": "La VM refuse la connexion SSH.",
        "unreachable": "La VM ne répond pas (délai dépassé ou pas de route).",
        "key_denied": "La VM refuse votre clé SSH.",
        "tailscale_policy": "La politique Tailscale SSH refuse cet utilisateur.",
        "host_key_unknown": "Ce Mac ne connaît pas encore l’empreinte SSH de la VM.",
        "host_key_changed": "L’empreinte SSH de la VM a changé : VM reconstruite, ou usurpation.",
        "sftp_unavailable": "La VM ne répond pas au protocole SFTP.",
        "no_posix_rename": "Le serveur SFTP ne garantit pas l’enregistrement atomique : certains éditeurs pourraient échouer.",
        "directory_missing": "Le dossier distant n’existe pas.",
        "directory_not_a_folder": "Ce chemin n’est pas un dossier.",
        "directory_not_writable": "Vous ne pouvez pas écrire dans ce dossier.",
        "directory_invalid": "Le chemin doit être absolu.",
        "outside_tailscale": "Cette machine ne fait pas partie de votre réseau Tailscale.",
    ]

    static func detail(_ item: OnboardCheckItem, host: String) -> String {
        if item.status == "ok" { return okLabels[item.id] ?? item.detail }
        return details[item.code] ?? item.detail
    }

    private static func quote(_ value: String) -> String {
        "'" + value.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func fix(_ item: OnboardCheckItem, host: String, remote: String) -> String? {
        guard item.status != "ok" else { return nil }
        let name = host.split(separator: "@").last.map(String.init) ?? host
        switch item.code {
        case "ssh_missing":
            return "Installez les outils en ligne de commande Apple : `xcode-select --install`."
        case "tailscale_cli_missing":
            return "Installez et ouvrez l’app Tailscale (tailscale.com/download/mac). Sans importance si votre VM est joignable autrement."
        case "tailscale_stopped":
            return "Ouvrez Tailscale dans la barre de menus et choisissez Connecter, puis réessayez."
        case "tailscale_login":
            return "Connectez-vous à Tailscale depuis la barre de menus (validez ce Mac si on vous le demande), puis réessayez."
        case "unresolved":
            return "Vérifiez l’orthographe. Pour une machine Tailscale, choisissez-la dans la liste et vérifiez que Tailscale est connecté."
        case "refused":
            return "Démarrez le serveur SSH sur la VM (par exemple `sudo systemctl enable --now sshd`) ou vérifiez le port dans votre configuration SSH."
        case "unreachable":
            return "Connectez Tailscale sur ce Mac et sur la VM (elle doit apparaître en ligne dans `tailscale status`), puis réessayez."
        case "key_denied":
            return "Installez votre clé publique sur la VM, une seule fois : `ssh-copy-id \(host)` (le mot de passe de la VM est demandé une fois). Pas encore de clé ? Créez-en une d’abord : `ssh-keygen -t ed25519`."
        case "tailscale_policy":
            return "Votre politique Tailscale n’autorise pas cet utilisateur sur cette machine. Utilisez un utilisateur permis par votre ACL (règles SSH de la console d’administration Tailscale), ou un compte avec une clé SSH classique."
        case "host_key_unknown":
            return "Connectez-vous une fois et confirmez l’empreinte : `ssh \(host)` (répondez `yes` seulement si vous reconnaissez la machine), puis réessayez."
        case "host_key_changed":
            return "N’avancez pas à l’aveugle. Si vous avez reconstruit la VM, supprimez l’ancienne empreinte avec `ssh-keygen -R \(name)` puis reconnectez-vous ; sinon enquêtez avant de lui faire confiance."
        case "sftp_unavailable":
            return "Activez le sous-système SFTP sur la VM : ajoutez `Subsystem sftp internal-sftp` (ou le chemin du sftp-server d’OpenSSH) à /etc/ssh/sshd_config, puis rechargez sshd."
        case "no_posix_rename":
            return "Utilisez le `sftp-server` d’OpenSSH sur la VM (pas un serveur SFTP minimal)."
        case "directory_missing":
            return "Créez-le sur la VM : `mkdir -p \(quote(remote))`."
        case "directory_not_a_folder":
            return "Choisissez un dossier existant."
        case "directory_not_writable":
            return "Donnez-vous les droits d’écriture (par exemple `sudo chown $USER \(quote(remote))`) ou choisissez un dossier qui vous appartient."
        case "directory_invalid":
            return "Donnez un chemin absolu, par exemple /home/vous/projet."
        case "outside_tailscale":
            return "Sans importance si la VM est joignable autrement ; pour un lien privé, ajoutez-la à votre réseau Tailscale."
        default:
            return item.fix
        }
    }
}
