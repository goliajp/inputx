// swift-tools-version:5.9
import PackageDescription

// Shared Apple-platform Swift layer for the Inputx IME. Consumed by both
// the iOS app (UIKit / SwiftUI keyboard extension + container app) and the
// macOS IME (InputMethodKit + AppKit menubar). Anything Apple-specific
// that is NOT host-UI-specific lives here:
//
//   - `InputxCoreC` — system library target wrapping the cbindgen-emitted
//     `core/include/inputx_core.h`. Links `libinputx_core.{a,dylib}` built
//     by the Rust `inputx-core-ffi` crate. Library search path is set by
//     the consumer (iOS Xcode build settings; macOS build.sh `-L`).
//
//   - `InputxKit` — type-safe Swift wrappers (`InputxSession`,
//     `InputxL0Storage`, `InputxLocale`, `InputxSettings`) over the C ABI.
//     Pure Foundation; no UIKit/AppKit/SwiftUI/InputMethodKit imports.
//
// Per-host UI glue (the iOS keyboard extension, the Mac IMK controller,
// future web/Linux/Windows shims) consumes `InputxKit` but lives in its
// own platform directory.
let package = Package(
    name: "InputxKit",
    platforms: [
        .iOS(.v16),
        .macOS(.v13),
    ],
    products: [
        // Static library — Mac IME's build.sh links via `-lInputxKit`. iOS
        // Xcode (when integrated) embeds the SPM target directly via local
        // dep, ignoring this product type. `.static` is explicit so both
        // consumers see the same artifact shape.
        .library(name: "InputxKit", type: .static, targets: ["InputxKit"]),
    ],
    targets: [
        .systemLibrary(
            name: "InputxCoreC",
            path: "Sources/InputxCoreC"
        ),
        .target(
            name: "InputxKit",
            dependencies: ["InputxCoreC"],
            path: "Sources/InputxKit"
        ),
    ]
)
