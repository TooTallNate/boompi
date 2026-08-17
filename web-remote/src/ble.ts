// Web Bluetooth client for the boompid GATT control bridge
// (docs/BLE.md): the browser's device chooser is the discovery UI -
// requestDevice filters on the boompi service UUID, so only nearby
// boompis are listed.

import {
  CONTROL_CHAR_UUID,
  EVENTS_CHAR_UUID,
  Reassembler,
  SERVICE_UUID,
  chunkMessage,
} from "@boompi/ui/ble";
import type { ClientMessage } from "@boompi/ui/proto";

export interface BleEvents {
  onMessage(msg: Record<string, unknown> & { type: string }): void;
  onDisconnect(): void;
}

export class BleLink {
  private device: BluetoothDevice;
  private control: BluetoothRemoteGATTCharacteristic;
  private writeQueue: Promise<void> = Promise.resolve();

  private constructor(
    device: BluetoothDevice,
    control: BluetoothRemoteGATTCharacteristic,
  ) {
    this.device = device;
    this.control = control;
  }

  static supported(): boolean {
    return typeof navigator !== "undefined" && "bluetooth" in navigator;
  }

  /** Must be called from a user gesture (Web Bluetooth requirement).
   *  Resolves once notifications are flowing - the subscription
   *  greeting (hello + state) arrives via `onMessage`. */
  static async connect(handlers: BleEvents): Promise<BleLink> {
    const device = await navigator.bluetooth.requestDevice({
      filters: [{ services: [SERVICE_UUID] }],
    });
    if (!device.gatt) throw new Error("device has no GATT server");
    const server = await device.gatt.connect();
    const service = await server.getPrimaryService(SERVICE_UUID);
    const control = await service.getCharacteristic(CONTROL_CHAR_UUID);
    const events = await service.getCharacteristic(EVENTS_CHAR_UUID);

    const reassembler = new Reassembler();
    const decoder = new TextDecoder();
    events.addEventListener("characteristicvaluechanged", () => {
      const value = events.value;
      if (!value) return;
      const complete = reassembler.push(new Uint8Array(value.buffer, value.byteOffset, value.byteLength));
      if (!complete) return;
      try {
        handlers.onMessage(JSON.parse(decoder.decode(complete)));
      } catch {
        // Framing recovered a non-JSON blob; drop it.
      }
    });
    device.addEventListener("gattserverdisconnected", handlers.onDisconnect);
    await events.startNotifications();
    return new BleLink(device, control);
  }

  get name(): string {
    return this.device.name ?? "Boompi";
  }

  /** Chunk + write a ClientMessage. Writes are serialized through a
   *  queue: chunk order is the framing, so concurrent sends must not
   *  interleave. */
  send(msg: ClientMessage): void {
    const chunks = chunkMessage(new TextEncoder().encode(JSON.stringify(msg)));
    this.writeQueue = this.writeQueue
      .then(async () => {
        for (const chunk of chunks) {
          const buf = chunk.buffer.slice(chunk.byteOffset, chunk.byteOffset + chunk.byteLength) as ArrayBuffer;
          await this.control.writeValueWithResponse(buf);
        }
      })
      .catch((err) => {
        console.warn("BLE write failed", err);
      });
  }

  disconnect() {
    this.device.gatt?.disconnect();
  }
}
