// CoreBluetooth client for the boompid GATT control bridge
// (docs/BLE.md). Discovery is a scan filtered on the boompi service
// UUID; the connection speaks the same JSON protocol as the WebSocket,
// chunk-framed to the ATT MTU.
//
// Ordering matters everywhere here: chunk order IS the framing, so the
// central runs on the main queue and delegate callbacks use
// MainActor.assumeIsolated - a Task hop per notification could reorder
// chunks and permanently wedge reassembly (the multi-chunk state
// greeting would never complete, which looks like "stuck connecting").

import CoreBluetooth
import Foundation

public struct DiscoveredBox: Identifiable, Equatable {
    public let id: UUID
    public var name: String
    public var rssi: Int
}

public enum ConnectionPhase: Equatable {
    case idle
    case scanning
    case connecting(UUID)
    case connected
    /// Connection dropped; auto-reconnect pending for this box.
    case lost(UUID)
    case unavailable(String) // BT off / unauthorized
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
    @Published public private(set) var lastBoxID: UUID?

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
    private var retryNotBefore: [UUID: Date] = [:]
    /// Set while the user explicitly disconnected: suppresses the
    /// auto-reconnect until they pick a box again.
    private var userDisconnected = false

    private static let lastBoxKey = "boompi.lastBox"
    /// CB connect attempts pend forever; give up and rescan after this.
    private static let connectTimeoutSecs: UInt64 = 12

    public override init() {
        super.init()
        lastBoxID = UserDefaults.standard.string(forKey: Self.lastBoxKey).flatMap(UUID.init)
        // Main queue: delegate callbacks arrive on the main actor's
        // executor, so assumeIsolated below is sound and in-order.
        central = CBCentralManager(delegate: self, queue: .main)
    }

    public var caps: Set<String> { hello?.caps ?? [] }

    /// The name of a box, for status lines ("Reconnecting to X").
    public func boxName(_ id: UUID) -> String {
        discovered.first(where: { $0.id == id })?.name ?? "Boompi"
    }

    // MARK: - Public API

    public func startScanning() {
        guard central.state == .poweredOn else { return }
        if case .connected = phase { return }
        if case .idle = phase { phase = .scanning }
        central.scanForPeripherals(
            withServices: [CBUUID(string: BLE.serviceUUID)],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
    }

    public func connect(to id: UUID) {
        guard Date() >= retryNotBefore[id, default: .distantPast] else { return }
        guard let p = central.retrievePeripherals(withIdentifiers: [id]).first else { return }
        // Only one attempt at a time: drop any previous one cleanly.
        if let old = peripheral, old.identifier != id {
            central.cancelPeripheralConnection(old)
        }
        userDisconnected = false
        lastBoxID = id
        UserDefaults.standard.set(id.uuidString, forKey: Self.lastBoxKey)
        resetLink()
        phase = .connecting(id)
        peripheral = p
        p.delegate = self
        central.connect(p)
        armConnectTimeout(for: id)
    }

    public func disconnect() {
        userDisconnected = true
        connectTimeout?.cancel()
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
        for chunk in BLE.chunkMessage(data) {
            writeQueue.append(chunk)
        }
        pumpWrites()
    }

    // MARK: - Internals

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

    private func armConnectTimeout(for id: UUID) {
        connectTimeout?.cancel()
        connectTimeout = Task { [weak self] in
            try? await Task.sleep(nanoseconds: Self.connectTimeoutSecs * 1_000_000_000)
            guard !Task.isCancelled, let self else { return }
            if case .connecting(let pending) = self.phase, pending == id {
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
                self.phase = .unavailable("Bluetooth permission denied - enable it in Settings.")
            case .poweredOff:
                self.phase = .unavailable("Bluetooth is off.")
            default:
                break
            }
        }
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didDiscover peripheral: CBPeripheral,
        advertisementData: [String: Any],
        rssi RSSI: NSNumber
    ) {
        var name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String)
            ?? peripheral.name ?? "Boompi"
        // The GATT advert carries a distinct name ("Boompi Remote -
        // George's") so iOS Settings can tell it from the A2DP entry;
        // in our own list the speaker name alone reads better.
        if name.hasPrefix("Boompi Remote - ") {
            name = String(name.dropFirst("Boompi Remote - ".count))
        }
        MainActor.assumeIsolated {
            let id = peripheral.identifier
            if let i = self.discovered.firstIndex(where: { $0.id == id }) {
                self.discovered[i].name = name
                self.discovered[i].rssi = RSSI.intValue
            } else {
                self.discovered.append(DiscoveredBox(id: id, name: name, rssi: RSSI.intValue))
            }
            guard !self.userDisconnected, self.peripheral == nil else { return }
            switch self.phase {
            // The common case: one boompi, seen before - just connect.
            case .scanning, .idle where id == self.lastBoxID:
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
            let id = peripheral.identifier
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
            self.phase = .lost(peripheral.identifier)
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
