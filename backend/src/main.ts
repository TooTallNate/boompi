import i2c from 'i2c-bus';
import WebSocket from 'ws';
import dbus from 'dbus-next';
import createDebug from 'debug';
import { uptime } from 'os';
import { INA260, CONFIGURATION_REGISTER } from '@tootallnate/ina260';

import * as system from './system';
import { Bluetooth, BluetoothPlayer } from './bluetooth';
import { startCava } from './cava';

const debug = createDebug('boompi:backend:main');

async function main() {
	const bus = dbus.systemBus();
	const i2cBus = i2c.openSync(1);
	const ina = new INA260(i2cBus, 0x40);
	const wss = new WebSocket.Server({ port: 3001 });
	const cava = await startCava({
		bars: 10,
		bitFormat: 16
	});
	const prettyHostname = await system.getPrettyHostname();

	let player: BluetoothPlayer | null = null;

	// Write to the Configuration Register
	// 0x4427 means 16 averages, 1.1ms conversion time, shunt and bus continuous
	await ina.writeRegister(CONFIGURATION_REGISTER, 0x4427);

	function onMessage(message: string) {
		const data = JSON.parse(message);
		debug('Received WebSocket Message: %O', data);
		if (typeof data.volume === 'number') {
			system.setVolume(data.volume);
			player?.setVolume(data.volume);
		}
		if (data.play) {
			player?.play();
		}
		if (data.pause) {
			player?.pause();
		}
		if (data.rewind) {
			player?.previous();
		}
		if (data.fastForward) {
			player?.next();
		}
		if (typeof data.batteryFastPoll === 'boolean') {
			batteryPollingInterval = data.batteryFastPoll ? 1000 : 1000 * 30;
		}
	}

	function onClose() {
		debug('WebSocket disconnected');
	}

	wss.on('connection', (ws: WebSocket) => {
		debug('WebSocket connected');
		ws.on('message', onMessage);
		ws.on('close', onClose);
		if (player) {
			player.getVolume().then((volume) => {
				if (!player) return;
				ws.send(JSON.stringify({
					prettyHostname,
					bluetoothName: player.name,
					volume,
					uptime: uptime()
				}));
			});
		} else {
			ws.send(JSON.stringify({
				prettyHostname,
				bluetoothName: null,
				uptime: uptime()
			 }));
		}
	});

	let cavaExtra: Buffer | undefined;
	cava.stream.on('data', (data: Buffer) => {
		const buf = cavaExtra ? Buffer.concat([cavaExtra, data]) : data;
		const numPages = (buf.length / cava.pageSize) | 0;
		const numBytes = numPages * cava.pageSize;
		const leftover = buf.length - numBytes;
		cavaExtra = leftover > 0 ? buf.slice(numBytes) : undefined;

		// Only broadcast if non-empty
		let isEmpty = true;
		for (let i = 0; i < buf.length; i++) {
			if (buf[i] !== 0) {
				isEmpty = false;
				break;
			}
		}

		if (!isEmpty) {
			const uint8Array = buf.buffer.slice(buf.byteOffset, buf.byteLength + buf.length);
			broadcast(uint8Array, true);
		}
	});

	function broadcast(obj: any, binary = false) {
		debug('Broadcasting WebSocket message: %o', obj);
		const data = JSON.stringify(obj);
		for (const client of wss.clients) {
			if (client.readyState === WebSocket.OPEN) {
				client.send(data, { binary });
			}
		}
	}

	let batteryPollingInterval = 30 * 1000;
	(async () => {
		while (true) {
			const [voltage, current, power] = await Promise.all([
				ina.readVoltage(),
				ina.readCurrent(),
				ina.readPower(),
			]);
			const MAX_VOLTAGE = 24.98;
			const MIN_VOLTAGE = 18;
			const percentage =
				(voltage - MIN_VOLTAGE) / (MAX_VOLTAGE - MIN_VOLTAGE);
			broadcast({
				battery: { voltage, current, power, percentage, date: Date.now() },
			});
			await new Promise((r) => setTimeout(r, batteryPollingInterval));
		}
	})();

	const bt = new Bluetooth(bus);

	function onBluetoothUpdate(event: any) {
		broadcast(event);
	}

	function onBluetoothVolume(volume: number) {
		system.setVolume(volume);
		broadcast({ volume });
	}

	function onBluetoothDisconnect(this: BluetoothPlayer) {
		debug('Bluetooth device disconnected', this.name);
		player = null;
		broadcast({ bluetoothName: null });
	}

	bt.on('connect', async (p: BluetoothPlayer) => {
		player = p;
		const { name: bluetoothName } = p;
		//const vol = await p.getVolume();
		//system.setVolume(vol);
		//debug(
		//	'Bluetooth device connected %o (volume = %d)',
		//	bluetoothName,
		//	vol
		//);
		//broadcast({ bluetoothName, volume: vol });
		broadcast({ bluetoothName });

		// Remove previous listeners to ensure these
		// handlers are only invoked once per event
		player.removeListener('update', onBluetoothUpdate);
		player.removeListener('volume', onBluetoothVolume);
		player.removeListener('disconnect', onBluetoothDisconnect);

		player.on('update', onBluetoothUpdate);
		player.on('volume', onBluetoothVolume);
		player.on('disconnect', onBluetoothDisconnect);
	});
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
