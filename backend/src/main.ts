import i2c from 'i2c-bus';
import WebSocket from 'ws';
import dbus from 'dbus-next';
import createDebug from 'debug';

import * as system from './system';
import { INA260, CONFIGURATION_REGISTER } from './ina260';
import { Bluetooth, BluetoothPlayer } from './bluetooth';

const debug = createDebug('boompi:backend:main');

async function main() {
	const bus = dbus.systemBus();
	const i2cBus = i2c.openSync(1);
	const ina = new INA260(i2cBus, 0x40);
	const wss = new WebSocket.Server({ port: 3001 });
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
				ws.send(JSON.stringify({ bluetoothName: player.name, volume }));
			});
		} else {
			ws.send(JSON.stringify({ bluetoothName: null }));
		}
	});

	function broadcast(obj: any) {
		debug('Broadcasting WebSocket message: %o', obj);
		const data = JSON.stringify(obj);
		for (const client of wss.clients) {
			if (client.readyState === WebSocket.OPEN) {
				client.send(data);
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
			const MAX_VOLTAGE = 25;
			const MIN_VOLTAGE = 18;
			const percentage =
				(voltage - MIN_VOLTAGE) / (MAX_VOLTAGE - MIN_VOLTAGE);
			broadcast({
				battery: { voltage, current, power, percentage },
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
		const vol = await p.getVolume();
		system.setVolume(vol);
		debug(
			'Bluetooth device connected %o (volume = %d)',
			bluetoothName,
			vol
		);
		broadcast({ bluetoothName, volume: vol });

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
