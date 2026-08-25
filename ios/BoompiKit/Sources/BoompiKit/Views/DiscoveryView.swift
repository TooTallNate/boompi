import SwiftUI

/// Nearby boompis: BLE scan (filtered on the control service) plus
/// Bonjour browse (`_boompi._tcp`), so only real boxes appear. A box
/// seen on both transports is one row (the advertised box id joins
/// them; Wi-Fi wins), with the trailing icon showing the pipe - boxes
/// too old to advertise an id stay one row per pipe. Connection state
/// lives on each speaker's own row - no banner. The most recently
/// used box auto-connects the moment it's seen; the list is really
/// for first-run and multi-box homes.
struct DiscoveryView: View {
    @ObservedObject var client: BoompiClient

    var body: some View {
        List {
            if case .unavailable(let why) = client.phase {
                Section {
                    Label(why, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.secondary)
                }
            }

            Section {
                ForEach(client.discovered) { box in
                    SpeakerRow(client: client, box: box)
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
                Text("Any powered-on Boompi in Bluetooth range shows up here - no Wi-Fi or setup needed. Speakers on your Wi-Fi network appear too.")
            }
        }
        .navigationTitle("Boompi")
    }
}

private struct SpeakerRow: View {
    @ObservedObject var client: BoompiClient
    let box: DiscoveredBox

    private enum RowState {
        case none, connecting, reconnecting
    }

    private var rowState: RowState {
        switch client.phase {
        case .connecting(let id) where id == box.id: return .connecting
        case .lost(let id) where id == box.id: return .reconnecting
        default: return .none
        }
    }

    var body: some View {
        Button {
            client.connect(to: box.id)
        } label: {
            HStack {
                Image(systemName: "hifispeaker.fill")
                    .foregroundStyle(.tint)
                VStack(alignment: .leading, spacing: 2) {
                    Text(box.name)
                    switch rowState {
                    case .connecting:
                        Text("Connecting…")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    case .reconnecting:
                        Text("Connection lost - retrying when in range")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    case .none:
                        if box.id == client.lastBoxID {
                            Text("Last used")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                Spacer()
                if rowState != .none {
                    ProgressView()
                } else if let rssi = box.rssi {
                    SignalIcon(rssi: rssi)
                } else {
                    // Network box: mDNS has no signal strength.
                    Image(systemName: "wifi")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .buttonStyle(.plain)
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
