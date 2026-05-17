import Foundation
import Carbon

guard CommandLine.arguments.count >= 2 else {
    print("usage: register <bundle-path>")
    exit(1)
}
let path = CommandLine.arguments[1]
let url = URL(fileURLWithPath: path) as CFURL
let status = TISRegisterInputSource(url)
print("TISRegisterInputSource = \(status)")
exit(status == 0 ? 0 : 1)
