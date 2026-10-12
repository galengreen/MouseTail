// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "MouseTail",
    platforms: [.macOS(.v13)],
    dependencies: [
        // Updates: https://sparkle-project.org
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.6.0")
    ],
    targets: [
        .executableTarget(
            name: "MouseTail",
            dependencies: [.product(name: "Sparkle", package: "Sparkle")],
            path: "Sources/MouseTail",
            linkerSettings: [.unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])]
        )
    ],
    swiftLanguageModes: [.v5]
)
