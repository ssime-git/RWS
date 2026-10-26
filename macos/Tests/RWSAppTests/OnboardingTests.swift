import XCTest
@testable import RWSApp

final class OnboardingTests: XCTestCase {
    func testCheckCommandNeedsNoConfigurationAndOnlySendsRemoteWithAHost() {
        XCTAssertEqual(CLICommand.onboardCheck(host: nil, remote: nil).arguments,
                       ["onboard", "check", "--json"])
        XCTAssertEqual(CLICommand.onboardCheck(host: nil, remote: "/srv/p").arguments,
                       ["onboard", "check", "--json"], "--remote requires --host")
        XCTAssertEqual(
            CLICommand.onboardCheck(host: "dev@vm", remote: "/srv/p x").arguments,
            ["onboard", "check", "--host", "dev@vm", "--remote", "/srv/p x", "--json"])
    }

    func testAddCommandKeepsUserValuesAsSingleArgumentsAndNeverForcesAMountPath() {
        let config = URL(fileURLWithPath: "/tmp/config with spaces.json")
        XCTAssertEqual(
            CLICommand.onboardAdd(config: config, name: "work;x", host: "dev@vm", remotePath: "/srv/a b").arguments,
            ["--config", "/tmp/config with spaces.json", "onboard", "add", "work;x",
             "--host", "dev@vm", "--remote", "/srv/a b", "--json"])
    }

    private let checkJSON = """
    {"ok": false,
     "checks": [
       {"id": "mac.ssh", "status": "ok", "code": "ok", "detail": "OpenSSH_10"},
       {"id": "vm.reachable", "status": "fail", "code": "key_denied",
        "detail": "The VM rejected your SSH key.", "fix": "Install your public key"}],
     "machines": [
       {"name": "razer-1", "dns_name": "razer-1.tail.ts.net", "online": true, "os": "linux",
        "ips": ["100.106.23.6"]}]}
    """

    func testDecodesTheCLIReportIncludingSnakeCaseFieldsAndOptionalFixes() throws {
        let report = try OnboardReport.decode(Data(checkJSON.utf8))
        XCTAssertFalse(report.ok)
        XCTAssertNil(report.checks[0].fix)
        XCTAssertEqual(report.checks[1].code, "key_denied")
        XCTAssertEqual(report.machines.first?.dnsName, "razer-1.tail.ts.net")
        XCTAssertEqual(report.machines.first?.name, "razer-1")
        XCTAssertTrue(report.machines[0].online)
    }

    func testDecodesAnAddReportWithItsStepResults() throws {
        let json = """
        {"ok": false, "dry_run": false, "checks": [], "steps": [],
         "results": [{"id": "settings", "summary": "Use NFS", "status": "done", "output": ""},
                     {"id": "connect", "summary": "Mount", "status": "failed", "output": "boom"}],
         "mount": "/Users/me/RWS/demo"}
        """
        let report = try OnboardAddReport.decode(Data(json.utf8))
        XCTAssertEqual(report.results.map(\.status), ["done", "failed"])
        XCTAssertEqual(report.results[1].output, "boom")
        XCTAssertEqual(report.mount, "/Users/me/RWS/demo")
    }

    private func item(_ id: String, _ status: String, _ code: String, fix: String? = "english fix") -> OnboardCheckItem {
        OnboardCheckItem(id: id, status: status, code: code, detail: "english detail", fix: fix)
    }

    func testFrenchTextNamesTheHostInTheCommandsPeopleMustRun() {
        let key = item("vm.reachable", "fail", "key_denied")
        XCTAssertTrue(OnboardText.fix(key, host: "dev@vm", remote: "/srv/p")?.contains("ssh-copy-id dev@vm") == true)
        let unknown = item("vm.reachable", "fail", "host_key_unknown")
        XCTAssertTrue(OnboardText.fix(unknown, host: "dev@vm", remote: "/srv/p")?.contains("ssh dev@vm") == true)
        let changed = item("vm.reachable", "fail", "host_key_changed")
        let changedFix = OnboardText.fix(changed, host: "dev@vm", remote: "/srv/p") ?? ""
        XCTAssertTrue(changedFix.contains("ssh-keygen -R vm"), changedFix)
        XCTAssertTrue(changedFix.contains("pas"), "a changed key must warn, not read as routine")
        let missing = item("vm.directory", "fail", "directory_missing")
        XCTAssertTrue(OnboardText.fix(missing, host: "dev@vm", remote: "/srv/it's")?.contains("mkdir -p '/srv/it'\\''s'") == true)
    }

    func testOkChecksGetAFrenchLabelByIdAndUnknownCodesFallBackToTheCLITexts() {
        XCTAssertEqual(OnboardText.detail(item("mac.tailscale", "ok", "ok"), host: "h"), "Tailscale est connecté")
        XCTAssertEqual(OnboardText.detail(item("vm.sftp", "ok", "ok"), host: "h"), "La VM répond au protocole SFTP")
        let unknown = item("vm.reachable", "fail", "brand_new_code")
        XCTAssertEqual(OnboardText.detail(unknown, host: "h"), "english detail")
        XCTAssertEqual(OnboardText.fix(unknown, host: "h", remote: "/r"), "english fix")
        XCTAssertNil(OnboardText.fix(item("mac.ssh", "ok", "ok", fix: nil), host: "h", remote: "/r"))
    }

    func testNoFixTextEverReachesAnOKCheckEvenWhenTheCLISentOne() {
        XCTAssertNil(OnboardText.fix(item("vm.sftp", "ok", "ok", fix: "stray"), host: "h", remote: "/r"))
    }
}
