// The iOS app shell - everything real lives in BoompiKit so it builds
// and tests with the plain Swift toolchain.

import BoompiKit
import SwiftUI

@main
struct BoompiApp: App {
    var body: some Scene {
        WindowGroup {
            RootView()
        }
    }
}
