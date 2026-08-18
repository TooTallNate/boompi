import SwiftUI

/// The connected remote, structured like the native Settings app:
/// playback controls up top (this is a remote first), then Settings-
/// style drill-down rows - icon badge, title, current value, chevron.
/// Rows are capability-gated: a hard-wired box shows no Battery row,
/// a box without Wi-Fi hardware no Wi-Fi row (Hello.capabilities).
struct RemoteView: View {
    @ObservedObject var client: BoompiClient
    @State private var dragVolume: Double?
    @State private var lastVolumeSend = Date.distantPast
    @State private var trailingVolume: Task<Void, Never>?

    var body: some View {
        List {
            playbackSection

            Section {
                if client.caps.contains(Caps.wifi) {
                    NavigationLink {
                        WifiDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "wifi", color: .blue, title: "Wi-Fi") {
                            Text(client.state?.wifi?.connected ?? "Not Connected")
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                if client.caps.contains(Caps.bluetooth) {
                    NavigationLink {
                        BluetoothDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "bolt.horizontal", color: .blue, title: "Bluetooth") {
                            Text("On").foregroundStyle(.secondary)
                        }
                    }
                }
                if client.caps.contains(Caps.battery), let battery = client.state?.battery {
                    NavigationLink {
                        BatteryDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "battery.100percent", color: .green, title: "Battery") {
                            Text("\(Int(battery.percentage * 100))%")
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }

            Section {
                NavigationLink {
                    DisplayDetailView(client: client)
                } label: {
                    SettingsRow(icon: "sun.max.fill", color: .indigo, title: "Display")
                }
                if client.caps.contains(Caps.airplay) {
                    NavigationLink {
                        AirPlayDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "airplay.audio", color: .teal, title: "AirPlay")
                    }
                }
                if client.caps.contains(Caps.homeAssistant) {
                    NavigationLink {
                        HomeAssistantDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "house.fill", color: .orange, title: "Home Assistant") {
                            if client.state?.settings.mqttBroker.isEmpty == false {
                                Text("On").foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if client.caps.contains(Caps.games) {
                    NavigationLink {
                        GamesDetailView(client: client)
                    } label: {
                        SettingsRow(icon: "gamecontroller.fill", color: .purple, title: "Games") {
                            if client.state?.games?.running != nil {
                                Text("Playing").foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                NavigationLink {
                    GeneralDetailView(client: client)
                } label: {
                    SettingsRow(icon: "gear", color: .gray, title: "General") {
                        if client.state?.updates?.available != nil {
                            UpdateBadge()
                        }
                    }
                }
            }

            Section {
                Button("Disconnect", role: .destructive) {
                    client.disconnect()
                }
                .frame(maxWidth: .infinity)
            }
        }
        #if os(iOS)
        .listStyle(.insetGrouped)
        #endif
        .navigationTitle(client.state?.settings.name ?? "Boompi")
        #if os(iOS)
        .navigationBarTitleDisplayMode(.inline)
        #endif
    }

    // MARK: Playback

    private var playbackSection: some View {
        Section {
            if let track = client.state?.track, let title = track.title {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    if let artist = track.artist {
                        Text(artist)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }
                }
            }
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
            HStack(spacing: 12) {
                Image(systemName: "speaker.fill")
                    .foregroundStyle(.secondary)
                Slider(
                    value: Binding(
                        get: { dragVolume ?? client.state?.volume ?? 0 },
                        set: { v in
                            dragVolume = v
                            // Leading-edge immediate, then at most ~10/s
                            // with a trailing flush - mirrors the
                            // web/panel sliders.
                            let now = Date()
                            if now.timeIntervalSince(lastVolumeSend) >= 0.1 {
                                lastVolumeSend = now
                                client.send(.setVolume(v))
                            } else {
                                trailingVolume?.cancel()
                                trailingVolume = Task {
                                    try? await Task.sleep(nanoseconds: 100_000_000)
                                    guard !Task.isCancelled else { return }
                                    lastVolumeSend = Date()
                                    client.send(.setVolume(v))
                                }
                            }
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
    }
}

/// Settings-app row anatomy: colored rounded-square icon badge, title,
/// optional trailing value.
struct SettingsRow<Trailing: View>: View {
    let icon: String
    let color: Color
    let title: String
    @ViewBuilder var trailing: Trailing

    init(
        icon: String,
        color: Color,
        title: String,
        @ViewBuilder trailing: () -> Trailing = { EmptyView() }
    ) {
        self.icon = icon
        self.color = color
        self.title = title
        self.trailing = trailing()
    }

    var body: some View {
        HStack {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(.white)
                .frame(width: 28, height: 28)
                .background(RoundedRectangle(cornerRadius: 6).fill(color))
            Text(title)
            Spacer()
            trailing
        }
    }
}

/// The red numbered badge iOS Settings uses for a pending update.
struct UpdateBadge: View {
    var body: some View {
        Text("1")
            .font(.caption2.bold())
            .foregroundStyle(.white)
            .frame(width: 20, height: 20)
            .background(Circle().fill(.red))
    }
}
