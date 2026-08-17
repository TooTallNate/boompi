// BLE GATT transport constants + chunk framing, mirrored from
// rust/boompi-proto/src/lib.rs `pub mod ble` (see docs/BLE.md).
// Keep in sync by hand, like proto.ts.

export const SERVICE_UUID = "a5e90001-9c60-4b2a-a6ca-0d0a2b5f0e1f";
export const CONTROL_CHAR_UUID = "a5e90002-9c60-4b2a-a6ca-0d0a2b5f0e1f";
export const EVENTS_CHAR_UUID = "a5e90003-9c60-4b2a-a6ca-0d0a2b5f0e1f";
export const STATE_CHAR_UUID = "a5e90004-9c60-4b2a-a6ca-0d0a2b5f0e1f";

export const CHUNK_FIRST = 0x01;
export const CHUNK_LAST = 0x02;

/** Reassembly cap: protocol messages are small; anything bigger is a
 *  framing error, not a message. */
export const MAX_MESSAGE = 64 * 1024;

/** Conservative default chunk size (header + payload) when the ATT MTU
 *  is unknown. */
export const DEFAULT_CHUNK = 176;

/** Split a message into tagged chunks of at most `maxChunk` bytes
 *  (header byte included). */
export function chunkMessage(payload: Uint8Array, maxChunk = DEFAULT_CHUNK): Uint8Array[] {
  const body = Math.max(maxChunk, 2) - 1;
  const chunks: Uint8Array[] = [];
  for (let off = 0; off < payload.length; off += body) {
    const slice = payload.subarray(off, off + body);
    const buf = new Uint8Array(1 + slice.length);
    buf.set(slice, 1);
    chunks.push(buf);
  }
  if (chunks.length === 0) chunks.push(new Uint8Array([0]));
  chunks[0][0] |= CHUNK_FIRST;
  chunks[chunks.length - 1][0] |= CHUNK_LAST;
  return chunks;
}

/** Reassembles chunked messages; one instance per direction. Malformed
 *  sequences drop the partial message and resync on the next FIRST. */
export class Reassembler {
  private buf: number[] = [];
  private open = false;

  push(chunk: Uint8Array): Uint8Array | null {
    if (chunk.length === 0) return null;
    const flags = chunk[0];
    const payload = chunk.subarray(1);
    if (flags & CHUNK_FIRST) {
      this.buf = [];
      this.open = true;
    } else if (!this.open) {
      return null; // continuation without a start: drop
    }
    if (this.buf.length + payload.length > MAX_MESSAGE) {
      this.buf = [];
      this.open = false;
      return null;
    }
    this.buf.push(...payload);
    if (flags & CHUNK_LAST) {
      this.open = false;
      const out = new Uint8Array(this.buf);
      this.buf = [];
      return out;
    }
    return null;
  }
}
