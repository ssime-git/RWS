import XCTest
@testable import RWSApp

final class UpdateGuardTests: XCTestCase {
    func testStatusClassifierFailsClosedOnUnavailableOrUnrecognizedOutput() {
        XCTAssertEqual(MountStatusClassifier.classify("demo: disconnected"), .inactive)
        XCTAssertEqual(MountStatusClassifier.classify("demo: connected (verified RWS mount)"), .active)
        XCTAssertEqual(MountStatusClassifier.classify("demo: mounted (identity unverified; not managed by this configuration)"), .active)
        XCTAssertEqual(MountStatusClassifier.classify("demo: unavailable: mount table changed"), .unknown)
        XCTAssertEqual(MountStatusClassifier.classify("demo: disconnected\n[Output truncated]"), .unknown)
        XCTAssertEqual(MountStatusClassifier.classify("surprising output"), .unknown)
        XCTAssertEqual(MountStatusClassifier.classify(""), .inactive)
    }

    func testAllowsInstallOnlyWhenIdleAndMountStateKnownInactive() async {
        let guardState = UpdateGuard()
        await guardState.setMountState(.inactive)
        var allowed = await guardState.mayInstallUpdate()
        XCTAssertTrue(allowed)
        await guardState.beginOperation()
        allowed = await guardState.mayInstallUpdate()
        XCTAssertFalse(allowed)
        await guardState.endOperation()
        await guardState.setMountState(.active)
        allowed = await guardState.mayInstallUpdate()
        XCTAssertFalse(allowed)
        await guardState.setMountState(.unknown)
        allowed = await guardState.mayInstallUpdate()
        XCTAssertFalse(allowed)
    }
}
