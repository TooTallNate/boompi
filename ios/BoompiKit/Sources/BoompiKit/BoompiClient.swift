// Client for a boompi box over either transport: the BLE GATT control
// bridge (docs/BLE.md) or a WebSocket to boompid's /ws when phone and
// box share a Wi-Fi network (discovered via Bonjour, `_boompi._tcp`).
// Both pipes speak the same JSON protocol, so everything from the
// hello + state greeting onward is transport-agnostic; only discovery
// and framing differ (BLE chunks to the ATT MTU, WebSocket messages
// arrive whole).
//
// Ordering matters everywhere here: chunk order IS the framing, so the
// central runs on the main queue and delegate callbacks use
// MainActor.assumeIsolated - a Task hop per notification could reorder
// chunks and permanently wedge reassembly (the multi-chunk state
// greeting would never complete, which looks like "stuck connecting").

import CoreBluetooth
import Foundation

/// A box's identity, scoped by how it was discovered: CoreBluetooth
/// peripherals are phone-local UUIDs, Bonjour boxes carry the stable
/// box id (TXT `id`, "boompi-XXXX"). The same physical box seen both
/// ways is two entries - BLE adverts don't carry the box id, so the
/// link can't be unified until the protocol grows one.
public enum BoxID: Hashable {
    case ble(UUID)
    case network(String)

    public var isNetwork: Bool {
        if case .network = self { return true }
        return false
    }

    /// UserDefaults form ("ble:<uuid>" / "net:<key>").
    public var persisted: String {
        switch self {
        case .ble(let uuid): return "ble:\(uuid.uuidString)"
        case .network(let key): return "net:\(key)"
        }
    }

    /// Parses `persisted`; bare UUIDs (written by BLE-only builds of
    /// the app) load as `.ble` so the remembered box survives updating.
    public init?(persisted: String) {
        if persisted.hasPrefix("ble:"),
           let uuid = UUID(uuidString: String(persisted.dropFirst(4))) {
            self = .ble(uuid)
        } else if persisted.hasPrefix("net:"), persisted.count > 4 {
            self = .network(String(persisted.dropFirst(4)))
        } else if let uuid = UUID(uuidString: persisted) {
            self = .ble(uuid)
        } else {
            return nil
        }
    }
}

public struct DiscoveredBox: Identifiable, Equatable {
    public let id: BoxID
    public var name: String
    /// BLE signal strength; nil for network boxes (mDNS has no RSSI).
    public var rssi: Int?
}

public enum ConnectionPhase: Equatable {
    case idle
    case scanning
    case connecting(BoxID)
    case connected
    /// Connection dropped; auto-reconnect pending for this box.
    case lost(BoxID)
    case unavailable(String) // BT off / unauthorized (Wi-Fi still works)
}

@MainActor
public final class BoompiClient: NSObject, ObservableObject {
    @Published public private(set) var phase: ConnectionPhase = .idle
    @Published public private(set) var discovered: [DiscoveredBox] = []
    @Published public private(set) var hello: Hello?
    @Published public private(set) var state: BoxState?
    @Published public private(set) var wifiNetworks: [WifiNetwork] = []
    @Published public private(set) var pairing: Pairing?
    @Published public private(set) var btDevices: [BtDevice] = []

    /// Most recently connected box - auto-connected when seen again
    /// (the common case: a person has exactly one boompi).
    @Published public private(set) var lastBoxID: BoxID?

    private var central: CBCentralManager!
    private var peripheral: CBPeripheral?
    private var control: CBCharacteristic?
    private var reassembler = Reassembler()
    private var writeQueue: [Data] = []
    private var writeInFlight = false
    private var connectTimeout: Task<Void, Never>?
    /// After cancelPeripheralConnection, CB silently eats connect
    /// attempts issued before the cancel settles - retrying instantly
    /// on the next advert loops "Connecting/lost" forever. Cooldown
    /// before re-attempting the same box.
    private var retryNotBefore: [BoxID: Date] = [:]
    /// Set while the user explicitly disconnected: suppresses the
    /// auto-reconnect until they pick a box again.
    private var userDisconnected = false

    /// Wi-Fi side: Bonjour browser + the active WebSocket, if any.
    private let netDiscovery = NetworkDiscovery()
    private var ws: WebSocketTransport?
    /// Timer-driven reconnect for network boxes: unlike BLE (where a
    /// fresh advert signals the box is back), the avahi advert outlives
    /// boompid restarts, so reappearance can't be event-driven.
    private var netRetry: Task<Void, Never>?

    /// BLE scan results, merged with Bonjour results into `discovered`.
    private var bleFound: [DiscoveredBox] = []

    private static let lastBoxKey = "boompi.lastBox"
    /// CB connect attempts pend forever; give up and rescan after this.
    private static let connectTimeoutSecs: UInt64 = 12
    /// Retry cadence for a lost-but-still-advertised network box.
    private static let netRetrySecs: UInt64 = 4

    public override init() {
        super.init()
        lastBoxID = UserDefaults.standard.string(forKey: Self.lastBoxKey)
            .flatMap(BoxID.init(persisted:))
        // Main queue: delegate callbacks arrive on the main actor's
        // executor, so assumeIsolated below is sound and in-order.
        central = CBCentralManager(delegate: self, queue: .main)
        // Bonjour needs no permission gate equivalent to BT power-on;
        // browse from the start (the local-network prompt fires here).
        netDiscovery.onChange = { [weak self] boxes in
            self?.networkBoxesChanged(boxes)
        }
        netDiscovery.start()
    }

    public var caps: Set<String> { hello?.caps ?? [] }

    /// The name of a box, for status lines ("Reconnecting to X").
    public func boxName(_ id: BoxID) -> String {
        discovered.first(where: { $0.id == id })?.name ?? "Boompi"
    }

    // MARK: - Public API

    public func startScanning() {
        netDiscovery.start()
        guard central.state == .poweredOn else { return }
        if case .connected = phase { return }
        if case .idle = phase { phase = .scanning }
        central.scanForPeripherals(
            withServices: [CBUUID(string: BLE.serviceUUID)],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
    }

    public func connect(to id: BoxID) {
        switch id {
        case .ble(let uuid): connectBLE(uuid, id: id)
        case .network(let key): connectNetwork(key, id: id)
        }
    }

    public func disconnect() {
        userDisconnected = true
        connectTimeout?.cancel()
        closeNetwork()
        if let p = peripheral {
            central.cancelPeripheralConnection(p)
        }
        peripheral = nil
        resetLink()
        hello = nil
        state = nil
        phase = .idle
        startScanning()
    }

    /// Forget the remembered box (stops auto-connect on next launch).
    public func forgetLastBox() {
        lastBoxID = nil
        UserDefaults.standard.removeObject(forKey: Self.lastBoxKey)
    }

    public func send(_ msg: ClientMessage) {
        guard let data = try? msg.encode() else { return }
        if let ws {
            ws.send(data)
            return
        }
        for chunk in BLE.chunkMessage(data) {
            writeQueue.append(chunk)
        }
        pumpWrites()
    }

    // MARK: - Internals

    private func remember(_ id: BoxID) {
        userDisconnected = false
        lastBoxID = id
        UserDefaults.standard.set(id.persisted, forKey: Self.lastBoxKey)
    }

    private func connectBLE(_ uuid: UUID, id: BoxID) {
        guard Date() >= retryNotBefore[id, default: .distantPast] else { return }
        guard let p = central.retrievePeripherals(withIdentifiers: [uuid]).first else { return }
        // Only one link at a time, across both transports.
        closeNetwork()
        if let old = peripheral, old.identifier != uuid {
            central.cancelPeripheralConnection(old)
        }
        remember(id)
        resetLink()
        phase = .connecting(id)
        peripheral = p
        p.delegate = self
        central.connect(p)
        armConnectTimeout(for: id)
    }

    private func connectNetwork(_ key: String, id: BoxID) {
        guard let box = netDiscovery.boxes.first(where: { $0.key == key }) else { return }
        closeNetwork()
        if let old = peripheral {
            central.cancelPeripheralConnection(old)
            peripheral = nil
        }
        remember(id)
        resetLink()
        phase = .connecting(id)
        let ws = WebSocketTransport()
        self.ws = ws
        ws.onOpen = { [weak self] in
            // The box has no RTC; offer this phone's clock (ignored
            // whenever the box is NTP-synchronized).
            self?.send(.setTime(epochMs: UInt64(Date().timeIntervalSince1970 * 1000)))
        }
        ws.onMessage = { [weak self] data in
            self?.handle(data)
        }
        ws.onClose = { [weak self] _ in
            self?.networkLinkLost(id)
        }
        ws.connect(to: box.endpoint)
        armConnectTimeout(for: id)
    }

    /// Tear down the WebSocket side silently (no onClose callback).
    private func closeNetwork() {
        netRetry?.cancel()
        netRetry = nil
        ws?.close()
        ws = nil
    }

    private func networkLinkLost(_ id: BoxID) {
        ws = nil
        connectTimeout?.cancel()
        resetLink()
        hello = nil
        state = nil
        guard !userDisconnected else { return }
        phase = .lost(id)
        scheduleNetRetry(id)
    }

    /// Keep poking a lost network box while its advert is still
    /// visible: avahi advertises through boompid restarts and OTA
    /// reboots re-register within seconds of the port coming back.
    private func scheduleNetRetry(_ id: BoxID) {
        netRetry?.cancel()
        netRetry = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.netRetrySecs * 1_000_000_000)
            guard !Task.isCancelled, let self else { return }
            guard case .lost(let lostID) = self.phase, lostID == id,
                  !self.userDisconnected else { return }
            if case .network(let key) = id,
               self.netDiscovery.boxes.contains(where: { $0.key == key }) {
                self.connect(to: id)
            } else {
                self.scheduleNetRetry(id) // advert gone; keep waiting
            }
        }
    }

    private func networkBoxesChanged(_ boxes: [NetworkBox]) {
        rebuildDiscovered()
        guard !userDisconnected, peripheral == nil, ws == nil else { return }
        guard let last = lastBoxID, case .network(let key) = last,
              boxes.contains(where: { $0.key == key }) else { return }
        switch phase {
        // The common case: one boompi, seen before - just connect.
        // (.unavailable = BT off; Wi-Fi still works.)
        case .scanning, .idle, .unavailable:
            connect(to: last)
        case .lost(let lostID) where lostID == last && netRetry == nil:
            connect(to: last)
        default:
            break
        }
    }

    private func rebuildDiscovered() {
        let network = netDiscovery.boxes.map {
            DiscoveredBox(id: .network($0.key), name: $0.name, rssi: nil)
        }
        discovered = network + bleFound
    }

    /// Per-connection transport state. Chunk framing carries no
    /// message ids: stale half-reassembled bytes or queued writes from
    /// a previous link must never leak into a new one.
    private func resetLink() {
        reassembler = Reassembler()
        writeQueue.removeAll()
        writeInFlight = false
        control = nil
        wifiNetworks = []
        pairing = nil
        btDevices = []
    }

    private func armConnectTimeout(for id: BoxID) {
        connectTimeout?.cancel()
        connectTimeout = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.connectTimeoutSecs * 1_000_000_000)
            guard !Task.isCancelled, let self else { return }
            if case .connecting(let pending) = self.phase, pending == id {
                if id.isNetwork {
                    self.closeNetwork()
                    self.resetLink()
                    self.phase = .lost(id)
                    self.scheduleNetRetry(id)
                    return
                }
                if let p = self.peripheral {
                    self.central.cancelPeripheralConnection(p)
                }
                self.peripheral = nil
                self.resetLink()
                // Give the async cancel time to settle before any
                // retry - connects issued mid-cancel vanish silently.
                self.retryNotBefore[id] = Date().addingTimeInterval(3)
                self.phase = .lost(id)
                self.startScanning()
                // Retry when the box advertises again (didDiscover).
            }
        }
    }

    private func pumpWrites() {
        guard !writeInFlight, let p = peripheral, let c = control,
              !writeQueue.isEmpty else { return }
        writeInFlight = true
        // .withResponse serializes chunks: order is the framing.
        p.writeValue(writeQueue.removeFirst(), for: c, type: .withResponse)
    }

    private func handle(_ data: Data) {
        guard let msg = try? ServerMessage.decode(data) else { return }
        switch msg {
        case .hello(let h): hello = h
        case .state(let s):
            state = s
            connectTimeout?.cancel()
            phase = .connected
        case .settings(let s): state?.settings = s
        case .volume(let v): state?.volume = v
        case .battery(let b): state?.battery = b
        case .wifi(let w): state?.wifi = w
        case .wifiNetworks(let n): wifiNetworks = n
        case .update(let u): state?.updates = u
        case .track(let t): state?.track = t
        case .games(let g): state?.games = g
        case .pairing(let p): pairing = p
        case .btDevices(let d): btDevices = d
        case .emojiFonts(let e): state?.emojiFonts = e
        case .diag(let d): state?.diag = d
        case .other: break
        }
    }
}

// MARK: - CBCentralManagerDelegate

extension BoompiClient: CBCentralManagerDelegate {
    public nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
        MainActor.assumeIsolated {
            switch central.state {
            case .poweredOn:
                self.startScanning()
            case .unauthorized:
                self.bluetoothUnavailable("Bluetooth permission denied - enable it in Settings.")
            case .poweredOff:
                self.bluetoothUnavailable("Bluetooth is off.")
            default:
                break
            }
        }
    }

    /// BT going away only matters when BT was the plan: an active or
    /// pending network link keeps its phase (Wi-Fi discovery and the
    /// WebSocket don't care about the radio).
    private func bluetoothUnavailable(_ why: String) {
        switch phase {
        case .idle, .scanning, .unavailable:
            phase = .unavailable(why)
        default:
            break
        }
        bleFound = []
        rebuildDiscovered()
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        var name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String)
            ?? peripheral.name ?? "Boompi"
        // The GATT advert carries a distinct name ("🎛️ George's") so
        // iOS Settings can tell it from the A2DP entry; in our own
        // list the speaker name alone reads better. Old boxes used a
        // longer text prefix.
        for prefix in ["\u{1F39B}\u{FE0F} ", "\u{1F39B} ", "Boompi Remote - "] {
            if name.hasPrefix(prefix) {
                name = String(name.dropFirst(prefix.count))
                break
            }
        }
        MainActor.assumeIsolated {
            let id = BoxID.ble(peripheral.identifier)
            if let i = self.bleFound.firstIndex(where: { $0.id == id }) {
                self.bleFound[i].name = name
                self.bleFound[i].rssi = RSSI.intValue
            } else {
                self.bleFound.append(DiscoveredBox(id: id, name: name, rssi: RSSI.intValue))
            }
            self.rebuildDiscovered()
            guard !self.userDisconnected, self.peripheral == nil, self.ws == nil
            else { return }
            switch self.phase {
            // The common case: one boompi, seen before - just connect.
            case .scanning, .idle:
                if id == self.lastBoxID { self.connect(to: id) }
            // A lost box is advertising again: reconnect (or after
            // the post-cancel cooldown expires).
            case .lost(let lostID) where lostID == id:
                let wait = self.retryNotBefore[id, default: .distantPast].timeIntervalSinceNow
                if wait <= 0 {
                    self.connect(to: id)
                } else {
                    Task { [weak self] in
                        try? await Task.sleep(nanoseconds: UInt64(wait * 1_000_000_000))
                        guard let self, case .lost(let l) = self.phase, l == id else { return }
                        self.connect(to: id)
                    }
                }
            default:
                break
            }
        }
    }

    public nonisolated func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        MainActor.assumeIsolated {
            peripheral.discoverServices([CBUUID(string: BLE.serviceUUID)])
        }
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        MainActor.assumeIsolated {
            let id = BoxID.ble(peripheral.identifier)
            // A stale disconnect from a link we already abandoned for
            // Wi-Fi must not clobber the network connection.
            guard self.ws == nil else { return }
            self.resetLink()
            self.hello = nil
            self.state = nil
            guard !self.userDisconnected else { return }
            self.phase = .lost(id)
            self.peripheral = nil
            // Rescan; reconnect fires from didDiscover when the box
            // advertises again (more reliable than a blind pending
            // connect against a box that may be rebooting).
            self.startScanning()
        }
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        MainActor.assumeIsolated {
            guard self.ws == nil else { return }
            self.phase = .lost(.ble(peripheral.identifier))
            self.peripheral = nil
            self.resetLink()
            self.startScanning()
        }
    }
}

// MARK: - CBPeripheralDelegate

extension BoompiClient: CBPeripheralDelegate {
    public nonisolated func peripheral(_ peripheral: CBPeripheral, didDiscoverServices error: Error?) {
        guard let service = peripheral.services?.first(
            where: { $0.uuid == CBUUID(string: BLE.serviceUUID) }) else { return }
        peripheral.discoverCharacteristics(
            [CBUUID(string: BLE.controlCharUUID), CBUUID(string: BLE.eventsCharUUID)],
            for: service
        )
    }

    public nonisolated func peripheral(
        _ peripheral: CBPeripheral,
        didDiscoverCharacteristicsFor service: CBService,
        error: Error?
    ) {
        let chars = service.characteristics ?? []
        let control = chars.first { $0.uuid == CBUUID(string: BLE.controlCharUUID) }
        let events = chars.first { $0.uuid == CBUUID(string: BLE.eventsCharUUID) }
        MainActor.assumeIsolated {
            self.control = control
            if let events {
                // Subscribing triggers the hello + state greeting.
                peripheral.setNotifyValue(true, for: events)
            }
            // The box has no RTC; offer this phone's clock (ignored
            // whenever the box is NTP-synchronized).
            self.send(.setTime(epochMs: UInt64(Date().timeIntervalSince1970 * 1000)))
        }
    }

    public nonisolated func peripheral(
        _ peripheral: CBPeripheral,
        didUpdateValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        guard let value = characteristic.value else { return }
        MainActor.assumeIsolated {
            if let message = self.reassembler.push(value) {
                self.handle(message)
            }
        }
    }

    public nonisolated func peripheral(
        _ peripheral: CBPeripheral,
        didWriteValueFor characteristic: CBCharacteristic,
        error: Error?
    ) {
        MainActor.assumeIsolated {
            self.writeInFlight = false
            self.pumpWrites()
        }
    }
}
