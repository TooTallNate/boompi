import WebSocket from 'ws';
import dbus from 'dbus-next';
import createDebug from 'debug';

import * as system from './system';
import { getBluetoothPlayerEvents, BluetoothPlayer2 } from './bluetooth';

const debug = createDebug('boompi:backend:main');

async function main() {
	const bus = dbus.systemBus();
	const wss = new WebSocket.Server({ port: 3001 });
	let player: BluetoothPlayer2 | null = null;

	function onMessage (message: string) {
		const data = JSON.parse(message);
		debug('Received WebSocket Message: %O', data);
/*
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
*/
	}

	function onClose() {
		debug('WebSocket disconnected');
	}

	wss.on('connection', (ws: WebSocket) => {
		debug('WebSocket connected');
		ws.on('message', onMessage);
		ws.on('close', onClose);
		console.log('sending', player);
		if (player) {
			ws.send(JSON.stringify({ bluetoothName: player.name }));
		}
	});

	function broadcast(obj: any) {
		debug('Broadcasting WebSocket message: %O', obj);
		const data = JSON.stringify(obj);
		for (const client of wss.clients) {
			if (client.readyState === WebSocket.OPEN) {
				client.send(data);
			}
		}
	}

	for await (const event of getBluetoothPlayerEvents(bus)) {
		console.log('event', event);
		console.log(event.connect);
		if (event.connect) {
			const { name: bluetoothName } = event.player
			debug('Bluetooth device connected %o', bluetoothName);
			player = event.player;
console.log(player);
			broadcast({ bluetoothName });
		} else if (event.disconnect) {
			debug('Bluetooth device disconnected %o', event.player.name);
			player = null;
			broadcast({ bluetoothName: null });
		//} else {
		//	if (typeof event.volume === 'number') {
		//		system.setVolume(event.volume);
		//	}
		//	broadcast(event);
		}
	}

	console.log('Done');
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
