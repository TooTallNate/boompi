import SwiftUI

/// Nearby boompis (scan filtered on the control service, so only real
/// boxes appear). The most recently used box auto-connects the moment
/// it's seen - the list is really for first-run and multi-box homes.
struct DiscoveryView: View {
    @ObservedObject var client: BoompiClient

    var body: some View {
        List {
            switch client.phase {
            case .unavailable(let why):
                Label(why, systemImage: "exclamationmark.triangle")
                    .foregroundStyle(.secondary)
            case .connecting(let name):
                HStack(spacing: 12) {
                    ProgressView()
                    Text("Connecting to \(name)…")
                }
            case .lost(let name):
                HStack(spacing: 12) {
                    ProgressView()
                    VStack(alignment: .leading) {
                        Text("\(name) is out of reach")
                        Text("Reconnecting automatically when it's back.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            default:
                EmptyView()
            }

            Section {
                ForEach(client.discovered) { box in
                    Button {
                        client.connect(to: box.id)
                    } label: {
                        HStack {
                            Image(systemName: "hifispeaker.fill")
                                .foregroundStyle(.tint)
                            VStack(alignment: .leading) {
                                Text(box.name)
                                if box.id == client.lastBoxID {
                                    Text("Last used")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            Spacer()
                            SignalIcon(rssi: box.rssi)
                        }
                    }
                    .buttonStyle(.plain)
                }
                if client.discovered.isEmpty {
                    HStack(spacing: 12) {
                        ProgressView()
                        Text("Looking for speakers nearby…")
                            .foregroundStyle(.secondary)
                    }
                }
            } header: {
                Text("Speakers")
            } footer: {
                Text("Any powered-on Boompi in Bluetooth range shows up here - no Wi-Fi or setup needed.")
            }
        }
        .navigationTitle("Boompi")
    }
}

struct SignalIcon: View {
    let rssi: Int

    var body: some View {
        // Rough RSSI bucketing; exact numbers don't matter for a
        // same-room speaker.
        let bars = rssi > -55 ? 3 : rssi > -70 ? 2 : 1
        Image(systemName: "cellularbars", variableValue: Double(bars) / 3.0)
            .foregroundStyle(.secondary)
    }
}
