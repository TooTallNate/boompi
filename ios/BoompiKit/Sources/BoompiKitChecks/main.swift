// Assertion checks for BoompiKit, as a plain executable: Xcode's test
// frameworks (XCTest / swift-testing) don't ship with the Command
// Line Tools, and this repo's Macs and CI should be able to validate
// the protocol + framing with nothing but the Swift toolchain.
//
//     swift run BoompiKitChecks

import Foundation
import BoompiKit

var failures = 0

func expect(
    _ cond: @autoclosure () -> Bool,
    _ label: String,
    file: String = #file,
    line: Int = #line
) {
    if cond() {
        print("ok - \(label)")
    } else {
        failures += 1
        print("FAIL - \(label) (\(file):\(line))")
    }
}

// MARK: Chunking

do {
    let msg = Data("{\"type\":\"play\"}".utf8)
    let chunks = BLE.chunkMessage(msg)
    expect(chunks.count == 1, "small message is one chunk")
    expect(chunks[0][0] == BLE.chunkFirst | BLE.chunkLast, "single chunk carries FIRST|LAST")
    var r = Reassembler()
    expect(r.push(chunks[0]) == msg, "single-chunk round trip")
}

do {
    let msg = Data((0..<1000).map { UInt8($0 % 256) })
    let chunks = BLE.chunkMessage(msg, maxChunk: 20)
    expect(chunks.count > 1, "large message splits")
    expect(chunks.allSatisfy { $0.count <= 20 }, "chunks respect the size bound")
    expect(chunks.first![0] & BLE.chunkFirst != 0, "first chunk flagged FIRST")
    expect(chunks.last![0] & BLE.chunkLast != 0, "last chunk flagged LAST")
    var r = Reassembler()
    var out: Data?
    for c in chunks {
        expect(out == nil, "no early completion")
        out = r.push(c)
    }
    expect(out == msg, "multi-chunk round trip")
}

do {
    let chunks = BLE.chunkMessage(Data())
    expect(chunks == [Data([BLE.chunkFirst | BLE.chunkLast])], "empty message is one empty chunk")
    var r = Reassembler()
    expect(r.push(chunks[0]) == Data(), "empty round trip")
}

do {
    var r = Reassembler()
    expect(r.push(Data([0x00, 1, 2, 3])) == nil, "continuation without FIRST drops")
    let msg = Data("ok".utf8)
    expect(r.push(BLE.chunkMessage(msg)[0]) == msg, "resyncs on the next FIRST")
}

do {
    let chunks = BLE.chunkMessage(Data([1, 2, 3]), maxChunk: 0)
    expect(chunks.allSatisfy { $0.count == 2 }, "tiny maxChunk clamps to header+1")
    var r = Reassembler()
    var out: Data?
    for c in chunks { out = r.push(c) ?? out }
    expect(out == Data([1, 2, 3]), "clamped round trip")
}

// MARK: Proto

func decode(_ json: String) throws -> ServerMessage {
    try ServerMessage.decode(Data(json.utf8))
}

do {
    let msg = try decode(
        #"{"type":"hello","proto_version":2,"name":"George's 🔊","version":"2.2.0","uptime_secs":42,"capabilities":["wifi","wifi_scan","games"]}"#
    )
    if case .hello(let h) = msg {
        expect(h.name == "George's 🔊", "hello name decodes (emoji intact)")
        expect(h.caps.contains(Caps.wifiScan), "explicit caps respected")
        expect(!h.caps.contains(Caps.battery), "absent caps stay absent")
    } else {
        expect(false, "hello decodes as .hello")
    }
} catch { expect(false, "hello decodes: \(error)") }

do {
    let msg = try decode(
        #"{"type":"hello","proto_version":2,"name":"Old Box","version":"2.0.0","uptime_secs":1}"#
    )
    if case .hello(let h) = msg {
        expect(h.caps.contains(Caps.battery), "legacy fallback includes battery")
        expect(!h.caps.contains(Caps.wifiScan), "legacy fallback excludes wifi_scan")
    } else {
        expect(false, "legacy hello decodes as .hello")
    }
} catch { expect(false, "legacy hello decodes: \(error)") }

do {
    let msg = try decode(#"{"type":"quantum_entangle","level":11}"#)
    if case .other(let t) = msg {
        expect(t == "quantum_entangle", "unknown types land in .other")
    } else {
        expect(false, "unknown type is .other")
    }
} catch { expect(false, "unknown type decodes: \(error)") }

do {
    let msg = try decode(
        #"{"type":"wifi_networks","networks":[{"ssid":"Simnet","signal":82,"security":"WPA2","in_use":true,"saved":true}]}"#
    )
    if case .wifiNetworks(let nets) = msg {
        expect(nets.count == 1 && nets[0].inUse, "wifi_networks decodes")
    } else {
        expect(false, "wifi_networks decodes")
    }
} catch { expect(false, "wifi_networks decodes: \(error)") }

do {
    let data = try ClientMessage.wifiConnect(ssid: "Simnet", psk: "hunter22").encode()
    let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    expect(obj?["type"] as? String == "wifi", "wifi connect: type tag")
    expect(obj?["action"] as? String == "connect", "wifi connect: action tag")
    expect(obj?["psk"] as? String == "hunter22", "wifi connect: psk carried")

    let time = try ClientMessage.setTime(epochMs: 1_786_951_412_000).encode()
    let tobj = try JSONSerialization.jsonObject(with: time) as? [String: Any]
    expect(
        (tobj?["epoch_ms"] as? NSNumber)?.uint64Value == 1_786_951_412_000,
        "set_time: epoch_ms survives"
    )
} catch { expect(false, "client message encoding: \(error)") }

do {
    let msg = try decode(
        #"{"type":"emoji_fonts","fonts":[{"id":"noto","label":"Noto Color","license":"OFL","installed":true,"active":true,"builtin":true,"size":0}],"downloading":null,"progress":null,"error":null}"#
    )
    if case .emojiFonts(let e) = msg {
        expect(e.fonts.count == 1 && e.fonts[0].active, "emoji_fonts decodes")
    } else {
        expect(false, "emoji_fonts decodes as .emojiFonts")
    }
} catch { expect(false, "emoji_fonts decodes: \(error)") }

do {
    let json = #"{"settings":{"name":"X","theme":"dark","clock_24h":false,"screensaver":"clock","screensaver_min":10,"update_channel":"edge","ui_scale":1.5,"visualizer_opacity":0.6,"online_art_fallback":true,"airplay_model":"","airplay_classic":false,"game_volume":0.8,"mqtt_broker":"","mqtt_username":"","mqtt_password":""},"volume":0.4}"#
    let state = try JSONDecoder().decode(BoxState.self, from: Data(json.utf8))
    expect(state.settings.uiScale == 1.5, "settings ui_scale decodes")
    expect(state.settings.gameVolume == 0.8, "settings game_volume decodes")
} catch { expect(false, "full settings decode: \(error)") }

if failures > 0 {
    print("\n\(failures) check(s) FAILED")
    exit(1)
}
print("\nall checks passed")
