// swift-tools-version: 5.9
import PackageDescription

// All app logic lives here (BLE transport, protocol, SwiftUI views) so
// it builds and tests with the plain Swift toolchain - no Xcode needed
// for development of everything except the final app shell. The iOS
// app target (ios/project.yml, XcodeGen) is a thin @main around this.
let package = Package(
    name: "BoompiKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "BoompiKit", targets: ["BoompiKit"])
    ],
    targets: [
        .target(name: "BoompiKit"),
        // Plain executable instead of a test target: XCTest/swift-testing
        // only ship with Xcode, and this must validate on CLT-only
        // machines and CI. Run: swift run BoompiKitChecks
        .executableTarget(name: "BoompiKitChecks", dependencies: ["BoompiKit"]),
    ]
)
