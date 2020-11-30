import createDebug from 'debug';
import { EventEmitter } from 'events';
import { basename, join } from 'path';
import { camelCase } from 'camel-case';
import { MessageBus, ProxyObject, Variant } from 'dbus-next';

const debug = createDebug('boompi:backend:bluetooth');

export async function getBluetoothPlayer(bus: MessageBus) {
	const obj = await bus.getProxyObject('org.bluez', '/org/bluez/hci0');
	let player = null;
	for (const node of obj.nodes) {
		player = await getPlayer(bus, node);
		if (player) break;
	}
	return player;
}

async function getPlayer(bus: MessageBus, node: string) {
	const obj = await bus.getProxyObject('org.bluez', node);
	const properties = obj.getInterface('org.freedesktop.DBus.Properties');
	const props = await properties.GetAll('org.bluez.MediaControl1');
	const [name, connected] = (
		await Promise.all([
			properties.Get('org.bluez.Device1', 'Alias'),
			properties.Get('org.bluez.Device1', 'Connected'),
		])
	).map((v) => v.value);
	const isPlayer = obj.nodes.some((node) =>
		basename(node).startsWith('player')
	);
	debug('Bluetooth Device: %o', { node, name, connected, isPlayer });
	if (!connected || !isPlayer) {
		return null;
	}
	return new Player(obj, name);
}

interface VariantMap<T = any> {
	[name: string]: Variant<T>;
}

export class Player extends EventEmitter {
	obj: ProxyObject;
	name: string;
	fdPromise: Promise<ProxyObject>;
	playerPromise: Promise<ProxyObject>;

	constructor(obj: ProxyObject, name: string) {
		super();
		this.obj = obj;
		this.name = name;

		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onPropertyChange);

		const fdNode = obj.nodes.find((node) => /^fd\d+$/.test(basename(node)));
		if (!fdNode) {
			throw new Error('Could not determind "fd" node');
		}
		debug('Using fd node: %o', fdNode);
		this.fdPromise = obj.bus.getProxyObject('org.bluez', fdNode);

		this.playerPromise = properties
			.Get('org.bluez.MediaControl1', 'Player')
			.then((p: Variant) => {
				debug('Using player node: %o', p.value);
				return obj.bus.getProxyObject('org.bluez', p.value);
			});

		this.initFd();
		this.initPlayer();
	}

	async initFd() {
		const fd = await this.fdPromise;
		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onFdPropertyChange);
	}

	async initPlayer() {
		const player = await this.playerPromise;
		const properties = player.getInterface(
			'org.freedesktop.DBus.Properties'
		);
		properties.on('PropertiesChanged', this.onPlayerPropertyChange);
	}

	onFdPropertyChange = (iface: string, changed: VariantMap) => {
		for (const prop of Object.keys(changed)) {
			if (prop === 'Volume') {
				this.emit('volume', changed[prop].value / 127);
			}
		}
	};

	onPlayerPropertyChange = (iface: string, changed: VariantMap) => {
		console.log('onPlayerPropertyChanged', iface, changed);
		const state: { [name: string]: any } = {};
		for (const prop of Object.keys(changed)) {
			const { value } = changed[prop];
			if (prop === 'Track') {
				if (value.Title) {
					state.track = value.Title.value;
				}
				if (value.Album) {
					state.album = value.Album.value;
				}
				if (value.Artist) {
					state.artist = value.Artist.value;
				}
				if (value.Duration) {
					state.duration = value.Duration.value;
				}
			} else {
				state[camelCase(prop)] = value;
			}
		}
		this.emit('state', state);
	};

	onPropertyChange = (iface: string, changed: VariantMap) => {
		console.log('onPropertyChanged', iface, changed);
		for (const prop of Object.keys(changed)) {
			if (prop === 'Player') {
				const playerNode = changed[prop].value;
				debug('Setting player: %o', playerNode);
				this.playerPromise = this.obj.bus.getProxyObject(
					'org.bluez',
					playerNode
				);
				this.initPlayer();
			}
		}
	};

	async getVolume(): Promise<number> {
		const fd = await this.fdPromise;
		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		const volume = await properties.Get(
			'org.bluez.MediaTransport1',
			'Volume'
		);
		return volume.value;
	}

	async setVolume(volume: number) {
		const fd = await this.fdPromise;
		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		const v = new Variant('q', Math.round(volume * 127));
		await properties.Set('org.bluez.MediaTransport1', 'Volume', v);
	}

	async getState() {
		const player = await this.playerPromise;
		const properties = player.getInterface(
			'org.freedesktop.DBus.Properties'
		);

		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		console.log(mediaPlayer);

		const props = await properties.GetAll('org.bluez.MediaPlayer1');
		const track = await properties.Get('org.bluez.MediaPlayer1', 'Track');
		return {
			status: props.Status.value,
			position: props.Position.value,
			duration: props.Track.value.Duration.value,
		};
	}

	async play() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Play();
	}

	async pause() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Pause();
	}

	async stop() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Stop();
	}

	async next() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Next();
	}

	async previous() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Previous();
	}

	async fastForward() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.FastForward();
	}

	async rewind() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Rewind();
	}
}
