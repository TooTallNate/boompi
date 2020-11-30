import WebSocket from 'ws';
import dbus from 'dbus-next';
import createDebug from 'debug';

import * as system from './system';
import { getBluetoothPlayer, Player } from './bluetooth';

const debug = createDebug('boompi:backend:main');

async function main() {
	const bus = dbus.systemBus();
	const wss = new WebSocket.Server({ port: 3001 });

	wss.on('connection', (ws: WebSocket) => {
		let player: Player | null = null;
		debug('Got WebSocket connection');

		/*
		getVolume().then((volume) => {
			ws.send(JSON.stringify({ volume }));
		});
		*/

		getBluetoothPlayer(bus).then(async (_player) => {
			if (!_player) return;
			player = _player;
			player.on('volume', (volume: number) => {
				ws.send(JSON.stringify({ volume }));
				system.setVolume(volume);
			});
			player.on('state', (state: any) => {
				ws.send(JSON.stringify(state));
			});
			const state = await player.getState();
			console.log(state);
			ws.send(JSON.stringify(state));
		});

		ws.on('message', (message: string) => {
			const data = JSON.parse(message);
			debug('Message: %o', data);
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
		});

		ws.on('close', () => {
			debug('WebSocket connection closed');
		});
	});
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
