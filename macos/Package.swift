// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "RWSApp",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "RWSApp", targets: ["RWSApp"])],
    dependencies: [
        .package(url: "https://github.com/sparkle-project/Sparkle", exact: "2.10.0")
    ],
    targets: [
        .target(name: "SidebarBridge"),
        .executableTarget(name: "RWSApp", dependencies: ["SidebarBridge", .product(name: "Sparkle", package: "Sparkle")]),
        .testTarget(name: "RWSAppTests", dependencies: ["RWSApp"])
    ]
)
