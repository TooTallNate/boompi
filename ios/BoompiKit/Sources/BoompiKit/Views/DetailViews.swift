// Drill-down detail screens, one per Settings-style row.

import SwiftUI

// MARK: - Wi-Fi

struct WifiDetailView: View {
    @ObservedObject var client: BoompiClient
    @State private var joiningSSID: String?
    @State private var psk = ""

    private var wifi: WifiState? { client.state?.wifi }
    private var canScan: Bool { client.caps.contains(Caps.wifiScan) }

    var body: some View {
        List {
            Section {
                if let connected = wifi?.connected {
                    HStack {
                        Text(connected)
                        Spacer()
                        if let ip = wifi?.ip {
                            Text(ip).font(.caption).foregroundStyle(.secondary)
                        }
                        Image(systemName: "checkmark").foregroundStyle(.blue)
                    }
                } else {
                    Text("Not connected").foregroundStyle(.secondary)
                }
            }

            Section {
                // No radio toggle while the hotspot is up: killing the
                // radio kills the hotspot (mirrors the web UI's guard).
                if wifi?.apActive != true {
                    Toggle("Wi-Fi", isOn: Binding(
                        get: { wifi?.enabled ?? false },
                        set: { client.send(.wifiRadio(enabled: $0)) }
                    ))
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
            }

            if canScan {
                Section("Networks") {
                    if client.wifiNetworks.isEmpty {
                        HStack(spacing: 12) {
                            ProgressView()
                            Text("Scanning…").foregroundStyle(.secondary)
                        }
                    }
                    ForEach(client.wifiNetworks) { net in
                        networkRow(net)
                    }
                }
            } else {
                Section {
                    Text("This speaker's software predates Wi-Fi management over Bluetooth - update it to scan and join networks from here.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .navigationTitle("Wi-Fi")
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
                Spacer()
                if net.inUse {
                    Image(systemName: "checkmark").foregroundStyle(.blue)
                } else if net.saved {
                    Text("Saved").font(.caption).foregroundStyle(.secondary)
                }
                if !net.security.isEmpty {
                    Image(systemName: "lock.fill")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
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

// MARK: - Bluetooth

struct BluetoothDetailView: View {
    @ObservedObject var client: BoompiClient

    private var pairing: Pairing? { client.pairing ?? client.state?.pairing }
    private var devices: [BtDevice] {
        client.btDevices.isEmpty ? (client.state?.btDevices ?? []) : client.btDevices
    }

    /// Grouped by what the device is - phones stream music,
    /// controllers play games, the rest just control.
    private var groupedDevices: [(String, [BtDevice])] {
        let buckets: [(String, (String) -> Bool)] = [
            ("Phones & Audio", { $0 == "phone" || $0 == "audio" }),
            ("Game Controllers", { $0 == "controller" }),
            ("Other Devices", { _ in true }),
        ]
        var seen = Set<String>()
        return buckets.compactMap { label, match in
            let group = devices.filter { !seen.contains($0.address) && match($0.kind ?? "other") }
            group.forEach { seen.insert($0.address) }
            return group.isEmpty ? nil : (label, group)
        }
    }

    var body: some View {
        List {
            Section {
                switch pairing?.state {
                case "discoverable":
                    HStack {
                        VStack(alignment: .leading) {
                            Text("Discoverable")
                            Text("Pick the speaker on the device you're pairing")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Cancel") { client.send(.pairing(action: "cancel")) }
                    }
                case "confirm":
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Pair with \(pairing?.deviceName ?? "device")?")
                        if let passkey = pairing?.passkey {
                            Text(String(format: "%06u", passkey))
                                .font(.title2.monospaced())
                        }
                        HStack {
                            Button("Pair") { client.send(.pairing(action: "confirm")) }
                                .buttonStyle(.borderedProminent)
                            Button("Reject") { client.send(.pairing(action: "reject")) }
                        }
                    }
                default:
                    Button {
                        client.send(.pairing(action: "enable"))
                    } label: {
                        Label("Pair a device", systemImage: "plus.circle")
                    }
                }
            }

            if devices.isEmpty {
                Section("My Devices") {
                    Text("No paired devices").foregroundStyle(.secondary)
                }
            }
            ForEach(groupedDevices, id: \.0) { label, group in
                Section(label) {
                    ForEach(group) { d in
                        HStack {
                            VStack(alignment: .leading) {
                                Text(d.name)
                                Text(d.connected ? "Connected" : "Not Connected")
                                    .font(.caption)
                                    .foregroundStyle(d.connected ? .green : .secondary)
                            }
                            Spacer()
                            Button(d.connected ? "Disconnect" : "Connect") {
                                client.send(.btDevice(
                                    address: d.address,
                                    action: d.connected ? "disconnect" : "connect"
                                ))
                            }
                            .font(.caption)
                        }
                    }
                }
            }
        }
        .navigationTitle("Bluetooth")
    }
}

// MARK: - Battery

struct BatteryDetailView: View {
    @ObservedObject var client: BoompiClient

    var body: some View {
        List {
            if let battery = client.state?.battery {
                Section {
                    HStack {
                        Text("\(Int(battery.percentage * 100))%")
                            .font(.largeTitle.bold())
                        Spacer()
                        Image(systemName: battery.charging ? "battery.100percent.bolt" : "battery.75percent")
                            .font(.title)
                            .foregroundStyle(battery.low ? .red : .green)
                    }
                    ProgressView(value: battery.percentage)
                        .tint(battery.low ? .red : .green)
                    Text(statusText(battery))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Section("Telemetry") {
                    LabeledContent("Voltage", value: String(format: "%.2f V", battery.voltage))
                    LabeledContent("Current", value: String(format: "%+.2f A", battery.current))
                    LabeledContent("Power", value: String(format: "%.1f W", battery.power))
                }
            } else {
                Text("No battery telemetry").foregroundStyle(.secondary)
            }
        }
        .navigationTitle("Battery")
    }

    private func statusText(_ b: Battery) -> String {
        if b.full { return "Full" }
        if b.charging { return "Charging" }
        if let secs = b.timeRemainingSecs {
            let h = secs / 3600
            let m = (secs % 3600) / 60
            return h > 0 ? "\(h)h \(m)m remaining" : "\(m)m remaining"
        }
        return "On battery"
    }
}

// MARK: - Display

struct DisplayDetailView: View {
    @ObservedObject var client: BoompiClient
    @State private var visualizerDrag: Double?

    private var settings: Settings? { client.state?.settings }

    var body: some View {
        List {
            if let settings {
                Section("Appearance") {
                    Picker("Panel theme", selection: Binding(
                        get: { settings.theme },
                        set: { client.send(.setSettings(["theme": $0])) }
                    )) {
                        Text("Dark").tag("dark")
                        Text("Light").tag("light")
                    }
                    Picker("Text size", selection: Binding(
                        get: { settings.uiScale },
                        set: { client.send(.setSettings(["ui_scale": $0])) }
                    )) {
                        ForEach([1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5], id: \.self) { sc in
                            Text("\(Int(sc * 100))%").tag(sc)
                        }
                    }
                    Toggle("24-hour clock", isOn: Binding(
                        get: { settings.clock24h },
                        set: { client.send(.setSettings(["clock_24h": $0])) }
                    ))
                }

                Section {
                    VStack(alignment: .leading) {
                        Text("Visualizer opacity: \(Int((visualizerDrag ?? settings.visualizerOpacity) * 100))%")
                        Slider(
                            value: Binding(
                                get: { visualizerDrag ?? settings.visualizerOpacity },
                                set: { visualizerDrag = $0 }
                            ),
                            in: 0.1...1.0
                        ) { editing in
                            if !editing, let v = visualizerDrag {
                                client.send(.setSettings(["visualizer_opacity": v]))
                                visualizerDrag = nil
                            }
                        }
                    }
                    Toggle(isOn: Binding(
                        get: { settings.onlineArtFallback },
                        set: { client.send(.setSettings(["online_art_fallback": $0])) }
                    )) {
                        VStack(alignment: .leading) {
                            Text("Fetch album art online")
                            Text("When a source sends no artwork (needs Wi-Fi)")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }

                if client.caps.contains(Caps.emojiFonts), client.state?.emojiFonts != nil {
                    Section {
                        NavigationLink("Emoji Style") {
                            EmojiFontsView(client: client)
                        }
                    }
                }
                if client.caps.contains(Caps.screensaver) {
                    Section("Screensaver") {
                        Picker("Screensaver", selection: Binding(
                            get: { settings.screensaver },
                            set: { client.send(.setSettings(["screensaver": $0])) }
                        )) {
                            Text("Off").tag("off")
                            Text("Clock").tag("clock")
                            Text("Matrix rain").tag("matrix")
                            Text("Album art").tag("art")
                        }
                        if settings.screensaver != "off" {
                            Picker("Start after", selection: Binding(
                                get: { settings.screensaverMin },
                                set: { client.send(.setSettings(["screensaver_min": $0])) }
                            )) {
                                ForEach([2, 5, 10, 20, 30, 60], id: \.self) { m in
                                    Text("\(m) min").tag(m)
                                }
                            }
                            Button("Preview on speaker") {
                                client.send(.previewScreensaver)
                            }
                        }
                    }
                }
            }
        }
        .navigationTitle("Display")
    }
}

// MARK: - Games

struct GamesDetailView: View {
    @ObservedObject var client: BoompiClient
    @State private var gameVolumeDrag: Double?

    var body: some View {
        List {
            Section {
                if let running = client.state?.games?.running {
                    HStack {
                        Label(running, systemImage: "gamecontroller.fill")
                            .lineLimit(1)
                        Spacer()
                        Button("Stop", role: .destructive) { client.send(.gameStop) }
                    }
                } else {
                    Label(
                        client.state?.games?.gamepad == true
                            ? "Gamepad connected - launch a game from the speaker's screen"
                            : "No game running",
                        systemImage: "gamecontroller"
                    )
                    .foregroundStyle(.secondary)
                }
            } footer: {
                Text("Upload ROMs from the speaker's settings page over Wi-Fi.")
            }

            if let settings = client.state?.settings {
                Section {
                    VStack(alignment: .leading) {
                        Text("Game volume: \(Int((gameVolumeDrag ?? settings.gameVolume) * 100))%")
                        Slider(
                            value: Binding(
                                get: { gameVolumeDrag ?? settings.gameVolume },
                                set: { gameVolumeDrag = $0 }
                            ),
                            in: 0...1
                        ) { editing in
                            if !editing, let v = gameVolumeDrag {
                                client.send(.setSettings(["game_volume": v]))
                                gameVolumeDrag = nil
                            }
                        }
                    }
                } footer: {
                    Text("Music ducks the game volume while both play.")
                }
            }

            if let games = client.state?.games?.games, !games.isEmpty {
                Section("Library") {
                    ForEach(games.filter { $0.system != "bios" }) { g in
                        Button {
                            client.send(.gameLaunch(system: g.system, file: g.file))
                        } label: {
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(g.name).lineLimit(1)
                                    Text(g.system.uppercased())
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                Image(systemName: "play.circle")
                                    .foregroundStyle(.tint)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .navigationTitle("Games")
    }
}

// MARK: - General (About + Software Update, like iOS Settings)

struct GeneralDetailView: View {
    @ObservedObject var client: BoompiClient
    @State private var name = ""
    @State private var confirmRestart = false

    var body: some View {
        List {
            Section("About") {
                VStack(alignment: .trailing, spacing: 2) {
                    HStack {
                        Text("Name")
                        Spacer()
                        TextField("Speaker name", text: $name)
                            .multilineTextAlignment(.trailing)
                            .autocorrectionDisabled()
                            .onChange(of: name) { _, v in
                                // Byte-capped: the Bluetooth advert has a
                                // hard budget (emoji are 4 bytes each).
                                var v = v
                                while v.trimmingCharacters(in: .whitespaces).utf8.count > Limits.speakerNameMaxBytes {
                                    v.removeLast()
                                }
                                if v != name { name = v }
                            }
                            .onSubmit {
                                let trimmed = name.trimmingCharacters(in: .whitespaces)
                                if !trimmed.isEmpty, trimmed != client.state?.settings.name {
                                    client.send(.setSettings(["name": trimmed]))
                                }
                            }
                    }
                    Text("\(name.trimmingCharacters(in: .whitespaces).utf8.count)/\(Limits.speakerNameMaxBytes) bytes")
                        .font(.caption2)
                        .foregroundStyle(
                            name.trimmingCharacters(in: .whitespaces).utf8.count >= Limits.speakerNameMaxBytes
                                ? AnyShapeStyle(.red) : AnyShapeStyle(.secondary)
                        )
                }
                if let model = client.hello?.model {
                    LabeledContent("Model", value: model)
                }
                LabeledContent("Software", value: client.hello?.version ?? "-")
                if let uptime = client.hello?.uptimeSecs {
                    LabeledContent("Uptime", value: "\(uptime / 3600)h \((uptime % 3600) / 60)m")
                }
                if let temp = client.state?.diag?.cpuTempC {
                    LabeledContent("CPU Temperature") {
                        Text(String(format: "%.1f °C", temp))
                            .foregroundStyle(temp >= 75 ? .red : .secondary)
                    }
                }
                if client.state?.diag?.throttled == true {
                    Label("CPU throttled - overheating or weak power supply", systemImage: "thermometer.high")
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            }

            if client.caps.contains(Caps.updates) {
                Section {
                    NavigationLink {
                        SoftwareUpdateView(client: client)
                    } label: {
                        HStack {
                            Text("Software Update")
                            Spacer()
                            if client.state?.updates?.available != nil {
                                UpdateBadge()
                            }
                        }
                    }
                }
            }

            Section {
                Button("Restart Speaker", role: .destructive) {
                    confirmRestart = true
                }
                .frame(maxWidth: .infinity)
            } footer: {
                Text("Orderly reboot - the speaker is back in about half a minute and this app reconnects automatically.")
            }
        }
        .navigationTitle("General")
        .confirmationDialog(
            "Restart the speaker?",
            isPresented: $confirmRestart,
            titleVisibility: .visible
        ) {
            Button("Restart", role: .destructive) { client.send(.reboot) }
        } message: {
            Text("Music stops and it's back in about 30 seconds.")
        }
        .onAppear { name = client.state?.settings.name ?? "" }
    }
}

struct SoftwareUpdateView: View {
    @ObservedObject var client: BoompiClient

    private var updates: UpdateState? { client.state?.updates }

    var body: some View {
        List {
            Section {
                if let u = updates {
                    if let applying = u.applying {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Installing \(applying)…")
                            ProgressView(value: u.progress ?? 0)
                        }
                    } else if let available = u.available {
                        VStack(alignment: .leading, spacing: 8) {
                            Text(available).font(.headline)
                            Button("Update Now") { client.send(.update(action: "apply")) }
                                .buttonStyle(.borderedProminent)
                        }
                    } else if u.checking {
                        HStack(spacing: 12) {
                            ProgressView()
                            Text("Checking for updates…")
                        }
                    } else {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(u.version)
                            Text("Your speaker is up to date")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                    if u.applying == nil {
                        Button("Check for Updates") { client.send(.update(action: "check")) }
                            .disabled(u.checking)
                    }
                    if let error = u.error {
                        Text(error).font(.caption).foregroundStyle(.red)
                    }
                }
            }

            if let settings = client.state?.settings {
                Section {
                    Toggle("Bleeding edge updates", isOn: Binding(
                        get: { settings.updateChannel == "edge" },
                        set: { client.send(.setSettings(["update_channel": $0 ? "edge" : "stable"])) }
                    ))
                } footer: {
                    Text("Follow every green dev build, not just tagged releases.")
                }
            }
        }
        .navigationTitle("Software Update")
    }
}


// MARK: - Emoji style

struct EmojiFontsView: View {
    @ObservedObject var client: BoompiClient

    private var emoji: EmojiFontsState? { client.state?.emojiFonts }

    var body: some View {
        List {
            Section {
                ForEach(emoji?.fonts ?? []) { f in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(f.label)
                            Text(f.license + (f.size > 0 && !f.installed ? " · \(f.size / 1_048_576) MB" : ""))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if f.active {
                            Image(systemName: "checkmark").foregroundStyle(.blue)
                        } else if f.installed {
                            Button("Use") {
                                client.send(.emojiFont(action: "select", id: f.id))
                            }
                            .font(.caption)
                            if !f.builtin {
                                Button(role: .destructive) {
                                    client.send(.emojiFont(action: "remove", id: f.id))
                                } label: {
                                    Image(systemName: "trash")
                                }
                                .font(.caption)
                            }
                        } else if emoji?.downloading == f.id {
                            ProgressView(value: emoji?.progress ?? 0)
                                .frame(width: 60)
                        } else {
                            Button("Download") {
                                client.send(.emojiFont(action: "download", id: f.id))
                            }
                            .font(.caption)
                            .disabled(emoji?.downloading != nil)
                        }
                    }
                }
            } footer: {
                Text("The font used for emoji on the speaker's screen. Switching restarts the panel briefly.")
            }
            if let error = emoji?.error {
                Text(error).font(.caption).foregroundStyle(.red)
            }
        }
        .navigationTitle("Emoji Style")
    }
}

// MARK: - AirPlay

struct AirPlayDetailView: View {
    @ObservedObject var client: BoompiClient

    private var settings: Settings? { client.state?.settings }

    private static let models: [(String, String, String)] = [
        ("Generic speaker", "", "hifispeaker"),
        ("HomePod mini", "AudioAccessory5,1", "homepodmini"),
        ("HomePod", "AudioAccessory1,1", "homepod"),
        ("Apple TV", "AppleTV14,1", "appletv"),
    ]

    var body: some View {
        List {
            if let settings {
                Section {
                    ForEach(Self.models, id: \.1) { label, value, symbol in
                        Button {
                            client.send(.setSettings(["airplay_model": value]))
                        } label: {
                            HStack {
                                Image(systemName: symbol)
                                    .frame(width: 28)
                                Text(label)
                                Spacer()
                                if settings.airplayModel == value {
                                    Image(systemName: "checkmark").foregroundStyle(.blue)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                    }
                } header: {
                    Text("Device icon")
                } footer: {
                    Text("Phones pick the icon in their AirPlay list from the advertised model. Senders may need to rediscover the speaker after changing this.")
                }

                Section {
                    Toggle("Classic AirPlay only", isOn: Binding(
                        get: { settings.airplayClassic },
                        set: { client.send(.setSettings(["airplay_classic": $0])) }
                    ))
                } footer: {
                    Text("The speaker's play/pause/next buttons only work over classic AirPlay. Trade: no multi-speaker audio while enabled.")
                }
            }
        }
        .navigationTitle("AirPlay")
    }
}

// MARK: - Home Assistant

struct HomeAssistantDetailView: View {
    @ObservedObject var client: BoompiClient
    @State private var broker: String?
    @State private var username: String?
    @State private var password: String?

    private var settings: Settings? { client.state?.settings }

    var body: some View {
        List {
            if let settings {
                Section {
                    LabeledContent("Broker") {
                        TextField("192.168.1.89:1883", text: Binding(
                            get: { broker ?? settings.mqttBroker },
                            set: { broker = $0 }
                        ))
                        .multilineTextAlignment(.trailing)
                        .autocorrectionDisabled()
                    }
                    LabeledContent("Username") {
                        TextField("", text: Binding(
                            get: { username ?? settings.mqttUsername },
                            set: { username = $0 }
                        ))
                        .multilineTextAlignment(.trailing)
                        .autocorrectionDisabled()
                    }
                    LabeledContent("Password") {
                        SecureField("", text: Binding(
                            get: { password ?? settings.mqttPassword },
                            set: { password = $0 }
                        ))
                        .multilineTextAlignment(.trailing)
                    }
                    Button("Save") {
                        client.send(.setSettings([
                            "mqtt_broker": broker ?? settings.mqttBroker,
                            "mqtt_username": username ?? settings.mqttUsername,
                            "mqtt_password": password ?? settings.mqttPassword,
                        ]))
                        broker = nil
                        username = nil
                        password = nil
                    }
                    .disabled(broker == nil && username == nil && password == nil)
                } footer: {
                    Text("Point the speaker at your MQTT broker and it appears in Home Assistant automatically: playback, volume, battery, pairing, and OS updates. Leave the broker empty to disable.")
                }
            }
        }
        .navigationTitle("Home Assistant")
    }
}
