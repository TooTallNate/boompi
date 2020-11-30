import WebSocket from 'ws';
import dbus from 'dbus-next';
import createDebug from 'debug';
import execa, { ExecaChildProcess } from 'execa';

import { getBluetoothPlayer, Player } from './bluetooth';

const debug = createDebug('boompi:backend:main');

async function main() {
	const bus = dbus.systemBus();

	let setVolumeChildProcess: ExecaChildProcess | null = null;
	const wss = new WebSocket.Server({ port: 3001 });

	/*
	function setVolume(value: number) {
		if (setVolumeChildProcess) {
			setVolumeChildProcess.cancel();
		}
		const volume = Math.floor(value * 100);
		debug('Setting volume to: %o', value);
		setVolumeChildProcess = execa('osascript', [
			'-e',
			`set volume output volume ${volume}`,
		]);
	}

	async function getVolume() {
		debug('Getting current volume');
		const { stdout } = await execa('osascript', [
			'-e',
			'output volume of (get volume settings)',
		]);
		return parseInt(stdout.trim(), 10) / 100;
	}
*/

	wss.on('connection', (ws: WebSocket) => {
		debug('Got WebSocket connection');

		/*
		getVolume().then((volume) => {
			ws.send(JSON.stringify({ volume }));
		});
*/
		let player: Player | null = null;

		getBluetoothPlayer(bus).then((_player) => {
			if (!_player) return;
			player = _player;
			player.on('volume', (volume: number) => {
				ws.send(JSON.stringify({ volume }));
			});
		});

		ws.on('message', (message: string) => {
			const data = JSON.parse(message);
			debug('Message: %o', data);
			if (typeof data.volume === 'number') {
				player?.setVolume(data.volume);
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
