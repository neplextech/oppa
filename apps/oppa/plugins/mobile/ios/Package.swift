// swift-tools-version:5.3

import PackageDescription

let package = Package(
    name: "tauri-plugin-oppa-mobile",
    platforms: [
        .macOS(.v10_13),
        .iOS(.v14),
    ],
    products: [
        .library(
            name: "tauri-plugin-oppa-mobile",
            type: .static,
            targets: ["tauri-plugin-oppa-mobile"]
        )
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api")
    ],
    targets: [
        .target(
            name: "tauri-plugin-oppa-mobile",
            dependencies: [
                .byName(name: "Tauri")
            ],
            path: "Sources"
        )
    ]
)
