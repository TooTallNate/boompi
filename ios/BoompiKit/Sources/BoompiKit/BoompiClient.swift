// CoreBluetooth client for the boompid GATT control bridge
// (docs/BLE.md). Discovery is a scan filtered on the boompi service
// UUID; the connection speaks the same JSON protocol as the WebSocket,
// chunk-framed to the ATT MTU.

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
    case connecting(String)
    case connected
    case lost(String)
    case unavailable(String) // BT off / unauthorized
}

@MainActor
public final class BoompiClient: NSObject, ObservableObject {
    @Published public private(set) var phase: ConnectionPhase = .idle
    @Published public private(set) var discovered: [DiscoveredBox] = []
    @Published public private(set) var hello: Hello?
    @Published public private(set) var state: BoxState?
    @Published public private(set) var wifiNetworks: [WifiNetwork] = []

    /// Most recently connected box - auto-connected when seen again
    /// (the common case: a person has exactly one boompi).
    @Published public private(set) var lastBoxID: UUID?

    private var central: CBCentralManager!
    private var peripheral: CBPeripheral?
    private var control: CBCharacteristic?
    private var reassembler = Reassembler()
    private var writeQueue: [Data] = []
    private var writeInFlight = false
    /// Set while the user explicitly disconnected: suppresses the
    /// auto-reconnect until they pick a box again.
    private var userDisconnected = false

    private static let lastBoxKey = "boompi.lastBox"

    public override init() {
        super.init()
        lastBoxID = UserDefaults.standard.string(forKey: Self.lastBoxKey).flatMap(UUID.init)
        central = CBCentralManager(delegate: self, queue: .main)
    }

    public var caps: Set<String> { hello?.caps ?? [] }

    // MARK: - Public API

    public func startScanning() {
        userDisconnected = false
        guard central.state == .poweredOn else { return }
        phase = .scanning
        central.scanForPeripherals(
            withServices: [CBUUID(string: BLE.serviceUUID)],
            options: [CBCentralManagerScanOptionAllowDuplicatesKey: false]
        )
    }

    public func connect(to id: UUID) {
        guard let p = central.retrievePeripherals(withIdentifiers: [id]).first else { return }
        userDisconnected = false
        lastBoxID = id
        UserDefaults.standard.set(id.uuidString, forKey: Self.lastBoxKey)
        central.stopScan()
        phase = .connecting(p.name ?? "Boompi")
        peripheral = p
        p.delegate = self
        central.connect(p)
    }

    public func disconnect() {
        userDisconnected = true
        if let p = peripheral {
            central.cancelPeripheralConnection(p)
        }
        peripheral = nil
        control = nil
        hello = nil
        state = nil
        wifiNetworks = []
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
            phase = .connected
        case .settings(let s): state?.settings = s
        case .volume(let v): state?.volume = v
        case .battery(let b): state?.battery = b
        case .wifi(let w): state?.wifi = w
        case .wifiNetworks(let n): wifiNetworks = n
        case .update(let u): state?.updates = u
        case .track(let t): state?.track = t
        case .other: break
        }
    }
}

// MARK: - CBCentralManagerDelegate

extension BoompiClient: CBCentralManagerDelegate {
    public nonisolated func centralManagerDidUpdateState(_ central: CBCentralManager) {
        Task { @MainActor in
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
        let name = (advertisementData[CBAdvertisementDataLocalNameKey] as? String)
            ?? peripheral.name ?? "Boompi"
        let id = peripheral.identifier
        let rssi = RSSI.intValue
        Task { @MainActor in
            if let i = self.discovered.firstIndex(where: { $0.id == id }) {
                self.discovered[i].name = name
                self.discovered[i].rssi = rssi
            } else {
                self.discovered.append(DiscoveredBox(id: id, name: name, rssi: rssi))
            }
            // The common case: one boompi, seen before - just connect.
            if !self.userDisconnected, self.peripheral == nil, id == self.lastBoxID {
                self.connect(to: id)
            }
        }
    }

    public nonisolated func centralManager(_ central: CBCentralManager, didConnect peripheral: CBPeripheral) {
        Task { @MainActor in
            peripheral.discoverServices([CBUUID(string: BLE.serviceUUID)])
        }
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didDisconnectPeripheral peripheral: CBPeripheral,
        error: Error?
    ) {
        Task { @MainActor in
            self.control = nil
            guard !self.userDisconnected else { return }
            self.phase = .lost(peripheral.name ?? "Boompi")
            self.state = nil
            self.hello = nil
            // Auto-reconnect: a pending connect survives out-of-range
            // and completes when the box is back.
            self.central.connect(peripheral)
        }
    }

    public nonisolated func centralManager(
        _ central: CBCentralManager,
        didFailToConnect peripheral: CBPeripheral,
        error: Error?
    ) {
        Task { @MainActor in
            self.phase = .lost(peripheral.name ?? "Boompi")
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
        Task { @MainActor in
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
        Task { @MainActor in
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
        Task { @MainActor in
            self.writeInFlight = false
            self.pumpWrites()
        }
    }
}
