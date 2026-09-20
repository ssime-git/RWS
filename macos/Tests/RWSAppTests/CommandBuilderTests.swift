import XCTest
@testable import RWSApp

final class CommandBuilderTests: XCTestCase {
    func testArgumentsKeepUserValuesAsIndividualArguments() {
        let config = URL(fileURLWithPath: "/tmp/config with spaces.json")
        XCTAssertEqual(
            CLICommand.add(config: config, name: "work;touch x", host: "dev@example", remotePath: "/srv/a b", mountPath: "/Volumes/a b").arguments,
            ["--config", "/tmp/config with spaces.json", "workspace", "add", "work;touch x", "--ssh", "dev@example", "--remote", "/srv/a b", "--mount", "/Volumes/a b"]
        )
    }

    func testOperationalCommandsAlwaysUseExplicitConfig() {
        let config = URL(fileURLWithPath: "/tmp/config.json")
        XCTAssertEqual(CLICommand.connect(config: config, workspace: "demo").arguments, ["--config", "/tmp/config.json", "connect", "demo"])
        XCTAssertEqual(CLICommand.disconnect(config: config, workspace: "demo").arguments, ["--config", "/tmp/config.json", "disconnect", "demo"])
        XCTAssertEqual(CLICommand.status(config: config, workspace: nil).arguments, ["--config", "/tmp/config.json", "status", "--no-probe"])
    }

    func testConnectRepairTargetsTheDeadMountExplicitly() {
        let config = URL(fileURLWithPath: "/tmp/config.json")
        XCTAssertEqual(
            CLICommand.connectRepair(config: config, workspace: "demo").arguments,
            ["--config", "/tmp/config.json", "connect", "demo", "--repair"]
        )
    }

    func testDeltaRulesRefreshOnlyTouchesExistingRules() {
        let config = URL(fileURLWithPath: "/tmp/config.json")
        XCTAssertEqual(
            CLICommand.deltaRulesRefresh(config: config).arguments,
            ["--config", "/tmp/config.json", "delta-rules", "--if-installed"]
        )
    }

    func testHookInstallTargetsTheActiveConfiguration() {
        let config = URL(fileURLWithPath: "/tmp/config.json")
        XCTAssertEqual(
            CLICommand.hookInstall(config: config).arguments,
            ["--config", "/tmp/config.json", "hook", "install"]
        )
    }
}
