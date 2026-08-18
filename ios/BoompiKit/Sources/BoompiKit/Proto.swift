// Hand-mirrored subset of the boompi protocol
// (rust/boompi-proto/src/lib.rs) - the pieces the iOS remote uses.
// JSON is snake_case on the wire.

import Foundation

// MARK: - Capabilities

public enum Caps {
    public static let wifi = "wifi"
    public static let wifiScan = "wifi_scan"
    public static let battery = "battery"
    public static let bluetooth = "bluetooth"
    public static let games = "games"
    public static let emojiFonts = "emoji_fonts"
    public static let updates = "updates"
    public static let screensaver = "screensaver"
    public static let homeAssistant = "home_assistant"
    public static let airplay = "airplay"

    /// What a box that predates the capabilities field supports:
    /// everything except the protocol Wi-Fi lifecycle.
    public static let legacy: Set<String> = [
        wifi, battery, bluetooth, games, emojiFonts, updates,
        screensaver, homeAssistant, airplay,
    ]
}

// MARK: - Server -> client

public struct Hello: Decodable, Equatable {
    public var protoVersion: UInt32
    public var name: String
    public var model: String?
    public var version: String
    public var uptimeSecs: UInt64
    public var capabilities: [String]?

    enum CodingKeys: String, CodingKey {
        case protoVersion = "proto_version"
        case name, model, version
        case uptimeSecs = "uptime_secs"
        case capabilities
    }

    /// The box's capability set, with the legacy fallback applied.
    public var caps: Set<String> {
        if let capabilities, !capabilities.isEmpty {
            return Set(capabilities)
        }
        return Caps.legacy
    }
}

public struct Settings: Decodable, Equatable {
    public var name: String
    public var theme: String
    public var clock24h: Bool
    public var screensaver: String
    public var screensaverMin: Int
    public var updateChannel: String

    enum CodingKeys: String, CodingKey {
        case name, theme, screensaver
        case clock24h = "clock_24h"
        case screensaverMin = "screensaver_min"
        case updateChannel = "update_channel"
    }
}

public struct Battery: Decodable, Equatable {
    public var voltage: Double
    public var current: Double
    public var power: Double
    /// 0.0-1.0 state of charge.
    public var percentage: Double
    public var charging: Bool
    public var full: Bool
    public var low: Bool
    public var timeRemainingSecs: Int?

    enum CodingKeys: String, CodingKey {
        case voltage, current, power, percentage, charging, full, low
        case timeRemainingSecs = "time_remaining_secs"
    }
}

public struct WifiState: Decodable, Equatable {
    public var supported: Bool
    public var enabled: Bool
    public var connected: String?
    public var ip: String?
    public var apActive: Bool
    public var apSsid: String?
    public var settingsUrl: String?

    enum CodingKeys: String, CodingKey {
        case supported, enabled, connected, ip
        case apActive = "ap_active"
        case apSsid = "ap_ssid"
        case settingsUrl = "settings_url"
    }
}

public struct WifiNetwork: Decodable, Equatable, Identifiable {
    public var ssid: String
    public var signal: Int
    public var security: String
    public var inUse: Bool
    public var saved: Bool

    public var id: String { ssid }

    enum CodingKeys: String, CodingKey {
        case ssid, signal, security, saved
        case inUse = "in_use"
    }
}

public struct UpdateState: Decodable, Equatable {
    public var version: String
    public var available: String?
    public var checking: Bool
    public var applying: String?
    public var progress: Double?
    public var error: String?
}

public struct TrackInfo: Decodable, Equatable {
    public var title: String?
    public var artist: String?
    public var album: String?
}

/// Decoded snapshot of the box (the `state` greeting + patched by
/// deltas as they arrive).
public struct BoxState: Decodable, Equatable {
    public var settings: Settings
    public var volume: Double
    public var battery: Battery?
    public var wifi: WifiState?
    public var updates: UpdateState?
    public var track: TrackInfo?
}

/// One incoming protocol message, decoded by its `type` tag. Unknown
/// types decode to `.other` and are ignored - future boxes will grow
/// messages this app hasn't heard of.
public enum ServerMessage {
    case hello(Hello)
    case state(BoxState)
    case settings(Settings)
    case volume(Double)
    case battery(Battery)
    case wifi(WifiState)
    case wifiNetworks([WifiNetwork])
    case update(UpdateState)
    case track(TrackInfo)
    case other(String)

    public static func decode(_ data: Data) throws -> ServerMessage {
        struct Tag: Decodable { let type: String }
        struct VolumeBody: Decodable { let level: Double }
        struct NetworksBody: Decodable { let networks: [WifiNetwork] }
        let dec = JSONDecoder()
        let tag = try dec.decode(Tag.self, from: data)
        switch tag.type {
        case "hello": return .hello(try dec.decode(Hello.self, from: data))
        case "state": return .state(try dec.decode(BoxState.self, from: data))
        case "settings": return .settings(try dec.decode(Settings.self, from: data))
        case "volume": return .volume(try dec.decode(VolumeBody.self, from: data).level)
        case "battery": return .battery(try dec.decode(Battery.self, from: data))
        case "wifi": return .wifi(try dec.decode(WifiState.self, from: data))
        case "wifi_networks":
            return .wifiNetworks(try dec.decode(NetworksBody.self, from: data).networks)
        case "update": return .update(try dec.decode(UpdateState.self, from: data))
        case "track": return .track(try dec.decode(TrackInfo.self, from: data))
        default: return .other(tag.type)
        }
    }
}

// MARK: - Client -> server

/// Outgoing messages, encoded as the protocol's internally-tagged
/// JSON. Built as dictionaries: the protocol is stringly-tagged and
/// this keeps the mirror small.
public enum ClientMessage {
    case play
    case pause
    case next
    case previous
    case setVolume(Double)
    case setTime(epochMs: UInt64)
    case setSettings([String: Any])
    case wifiScan
    case wifiConnect(ssid: String, psk: String?)
    case wifiDisconnect
    case wifiForget(ssid: String)
    case wifiAp(enabled: Bool)
    case update(action: String)

    public func encode() throws -> Data {
        var dict: [String: Any]
        switch self {
        case .play: dict = ["type": "play"]
        case .pause: dict = ["type": "pause"]
        case .next: dict = ["type": "next"]
        case .previous: dict = ["type": "previous"]
        case .setVolume(let level): dict = ["type": "set_volume", "level": level]
        case .setTime(let ms): dict = ["type": "set_time", "epoch_ms": ms]
        case .setSettings(let patch):
            dict = patch
            dict["type"] = "set_settings"
        case .wifiScan: dict = ["type": "wifi", "action": "scan"]
        case .wifiConnect(let ssid, let psk):
            dict = ["type": "wifi", "action": "connect", "ssid": ssid]
            if let psk { dict["psk"] = psk }
        case .wifiDisconnect: dict = ["type": "wifi", "action": "disconnect"]
        case .wifiForget(let ssid): dict = ["type": "wifi", "action": "forget", "ssid": ssid]
        case .wifiAp(let enabled): dict = ["type": "wifi", "action": "ap", "enabled": enabled]
        case .update(let action): dict = ["type": "update", "action": action]
        }
        return try JSONSerialization.data(withJSONObject: dict)
    }
}
