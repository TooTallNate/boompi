import SwiftUI

/// The connected remote. Every section is capability-gated: the box
/// declares what it can do in the connection greeting, so a hard-wired
/// box shows no Battery section and an old box shows no Wi-Fi scanning
/// (see Caps / Hello.caps).
struct RemoteView: View {
    @ObservedObject var client: BoompiClient
    @State private var dragVolume: Double?

    var body: some View {
        List {
            if let track = client.state?.track, track.title != nil {
                Section("Now playing") {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(track.title ?? "")
                            .font(.headline)
                        if let artist = track.artist {
                            Text(artist)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    }
                    transportButtons
                }
            } else {
                Section("Playback") { transportButtons }
            }

            Section("Volume") {
                HStack(spacing: 12) {
                    Image(systemName: "speaker.fill")
                        .foregroundStyle(.secondary)
                    Slider(
                        value: Binding(
                            get: { dragVolume ?? client.state?.volume ?? 0 },
                            set: { v in
                                dragVolume = v
                                client.send(.setVolume(v))
                            }
                        ),
                        in: 0...1
                    ) { editing in
                        if !editing { dragVolume = nil }
                    }
                    Image(systemName: "speaker.wave.3.fill")
                        .foregroundStyle(.secondary)
                }
            }

            if client.caps.contains(Caps.battery), let battery = client.state?.battery {
                BatterySection(battery: battery)
            }

            if client.caps.contains(Caps.wifi) {
                WifiSection(client: client)
            }

            if client.caps.contains(Caps.updates), let updates = client.state?.updates {
                SoftwareSection(client: client, updates: updates)
            }
        }
        .navigationTitle(client.state?.settings.name ?? "Boompi")
        .toolbar {
            Button("Disconnect") { client.disconnect() }
        }
    }

    private var transportButtons: some View {
        HStack {
            Spacer()
            Button { client.send(.previous) } label: {
                Image(systemName: "backward.fill").font(.title2)
            }
            Spacer()
            Button { client.send(.play) } label: {
                Image(systemName: "play.fill").font(.largeTitle)
            }
            Spacer()
            Button { client.send(.pause) } label: {
                Image(systemName: "pause.fill").font(.largeTitle)
            }
            Spacer()
            Button { client.send(.next) } label: {
                Image(systemName: "forward.fill").font(.title2)
            }
            Spacer()
        }
        .buttonStyle(.borderless)
    }
}

struct BatterySection: View {
    let battery: Battery

    var body: some View {
        Section("Battery") {
            HStack {
                Image(systemName: battery.charging ? "battery.100.bolt" : "battery.75")
                    .foregroundStyle(battery.low ? .red : .green)
                Text("\(Int(battery.percentage * 100))%")
                Text(statusText)
                    .foregroundStyle(.secondary)
                Spacer()
                Text(String(format: "%.1f W", battery.power))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            ProgressView(value: battery.percentage)
                .tint(battery.low ? .red : .green)
        }
    }

    private var statusText: String {
        if battery.full { return "Full" }
        if battery.charging { return "Charging" }
        if let secs = battery.timeRemainingSecs {
            let h = secs / 3600
            let m = (secs % 3600) / 60
            return h > 0 ? "\(h)h \(m)m left" : "\(m)m left"
        }
        return "On battery"
    }
}

struct WifiSection: View {
    @ObservedObject var client: BoompiClient
    @State private var joiningSSID: String?
    @State private var psk = ""

    private var wifi: WifiState? { client.state?.wifi }
    private var canScan: Bool { client.caps.contains(Caps.wifiScan) }

    var body: some View {
        Section {
            if let connected = wifi?.connected {
                Label {
                    Text(connected)
                    if let ip = wifi?.ip {
                        Text(ip).font(.caption).foregroundStyle(.secondary)
                    }
                } icon: {
                    Image(systemName: "wifi").foregroundStyle(.green)
                }
            } else {
                Label("Not connected", systemImage: "wifi.slash")
                    .foregroundStyle(.secondary)
            }

            Toggle(isOn: Binding(
                get: { wifi?.apActive ?? false },
                set: { client.send(.wifiAp(enabled: $0)) }
            )) {
                VStack(alignment: .leading) {
                    Text("Hotspot")
                    Text("The speaker broadcasts its own network")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            if canScan {
                ForEach(client.wifiNetworks) { net in
                    networkRow(net)
                }
            } else {
                Text("This speaker's software predates Wi-Fi management over Bluetooth - update it to scan and join networks from here.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Wi-Fi")
        }
        .onAppear {
            if canScan { client.send(.wifiScan) }
        }
    }

    @ViewBuilder
    private func networkRow(_ net: WifiNetwork) -> some View {
        Button {
            if net.saved || net.security.isEmpty {
                client.send(.wifiConnect(ssid: net.ssid, psk: nil))
            } else {
                joiningSSID = net.ssid
                psk = ""
            }
        } label: {
            HStack {
                Text(net.ssid)
                if !net.security.isEmpty {
                    Image(systemName: "lock.fill")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if net.inUse {
                    Text("Connected").font(.caption).foregroundStyle(.green)
                } else if net.saved {
                    Text("Saved").font(.caption).foregroundStyle(.secondary)
                }
                SignalIcon(rssi: net.signal - 100) // 0-100 -> dBm-ish
            }
        }
        .buttonStyle(.plain)
        .disabled(net.inUse)
        .alert("Join \(joiningSSID ?? "")", isPresented: Binding(
            get: { joiningSSID == net.ssid },
            set: { if !$0 { joiningSSID = nil } }
        )) {
            SecureField("Password", text: $psk)
            Button("Join") {
                client.send(.wifiConnect(ssid: net.ssid, psk: psk))
                joiningSSID = nil
            }
            Button("Cancel", role: .cancel) { joiningSSID = nil }
        }
    }
}

struct SoftwareSection: View {
    @ObservedObject var client: BoompiClient
    let updates: UpdateState

    var body: some View {
        Section("Software") {
            HStack {
                VStack(alignment: .leading) {
                    Text(updates.version)
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if updates.applying == nil {
                    if updates.available != nil {
                        Button("Update") { client.send(.update(action: "apply")) }
                            .buttonStyle(.borderedProminent)
                    }
                    Button(updates.available != nil ? "Re-check" : "Check") {
                        client.send(.update(action: "check"))
                    }
                    .disabled(updates.checking)
                }
            }
            if let progress = updates.progress, updates.applying != nil {
                ProgressView(value: progress)
            }
        }
    }

    private var detail: String {
        if let applying = updates.applying {
            return "Installing \(applying)… \(Int((updates.progress ?? 0) * 100))%"
        }
        if updates.checking { return "Checking…" }
        if let available = updates.available { return "\(available) is available" }
        return "Up to date"
    }
}
