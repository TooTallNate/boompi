// BLE chunk framing, mirrored from rust/boompi-proto/src/lib.rs
// `pub mod ble` (see docs/BLE.md). Keep in sync by hand, like the
// web's ble.ts.

import Foundation

public enum BLE {
    /// Primary GATT service advertised by boompid.
    public static let serviceUUID = "a5e90001-9c60-4b2a-a6ca-0d0a2b5f0e1f"
    /// Write: client -> server chunked JSON ClientMessage.
    public static let controlCharUUID = "a5e90002-9c60-4b2a-a6ca-0d0a2b5f0e1f"
    /// Notify: server -> client chunked JSON ServerMessage.
    /// Subscribing greets with hello + full state, then deltas.
    public static let eventsCharUUID = "a5e90003-9c60-4b2a-a6ca-0d0a2b5f0e1f"
    /// Read: full JSON state snapshot (GATT long-read).
    public static let stateCharUUID = "a5e90004-9c60-4b2a-a6ca-0d0a2b5f0e1f"

    public static let chunkFirst: UInt8 = 0x01
    public static let chunkLast: UInt8 = 0x02

    /// Reassembly cap: protocol messages are small; anything bigger is
    /// a framing error, not a message.
    public static let maxMessage = 64 * 1024

    /// Conservative default chunk size (header + payload) when the ATT
    /// MTU is unknown: fits the 185-byte MTU iOS negotiates by default.
    public static let defaultChunk = 176

    /// Split a message into tagged chunks of at most `maxChunk` bytes
    /// (header byte included).
    public static func chunkMessage(_ payload: Data, maxChunk: Int = defaultChunk) -> [Data] {
        let body = Swift.max(maxChunk, 2) - 1
        var chunks: [Data] = []
        var offset = 0
        while offset < payload.count {
            let end = Swift.min(offset + body, payload.count)
            var chunk = Data([0])
            chunk.append(payload[offset..<end])
            chunks.append(chunk)
            offset = end
        }
        if chunks.isEmpty {
            chunks.append(Data([0])) // empty message: one empty chunk
        }
        chunks[0][chunks[0].startIndex] |= chunkFirst
        let last = chunks.count - 1
        chunks[last][chunks[last].startIndex] |= chunkLast
        return chunks
    }
}

/// Reassembles chunked messages; one instance per direction. Malformed
/// sequences (missing FIRST, oversize) drop the partial message and
/// resynchronize on the next FIRST chunk.
public struct Reassembler {
    private var buffer = Data()
    private var open = false

    public init() {}

    public mutating func push(_ chunk: Data) -> Data? {
        guard let flags = chunk.first else { return nil }
        let payload = chunk.dropFirst()
        if flags & BLE.chunkFirst != 0 {
            buffer.removeAll(keepingCapacity: true)
            open = true
        } else if !open {
            return nil // continuation without a start: drop
        }
        if buffer.count + payload.count > BLE.maxMessage {
            buffer.removeAll(keepingCapacity: false)
            open = false
            return nil
        }
        buffer.append(payload)
        if flags & BLE.chunkLast != 0 {
            open = false
            let out = buffer
            buffer = Data()
            return out
        }
        return nil
    }
}
