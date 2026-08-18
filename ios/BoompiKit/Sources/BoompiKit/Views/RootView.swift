import SwiftUI

/// App entry: discovery until a box is connected, then the remote.
public struct RootView: View {
    @StateObject private var client = BoompiClient()

    public init() {}

    public var body: some View {
        NavigationStack {
            if case .connected = client.phase, client.state != nil {
                RemoteView(client: client)
            } else {
                DiscoveryView(client: client)
            }
        }
    }
}
