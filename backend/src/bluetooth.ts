import createDebug from 'debug';
import { basename, join } from 'path';
import { camelCase } from 'camel-case';
import { EventEmitter, once } from 'events';
import { MessageBus, ProxyObject, Variant } from 'dbus-next';

const debug = createDebug('boompi:backend:bluetooth');

export class Bluetooth extends EventEmitter {
	bus: MessageBus;
	hciPromise: Promise<ProxyObject>;
	players: Map<string, BluetoothPlayer>;

	constructor(bus: MessageBus) {
		super();
		this.bus = bus;
		this.players = new Map();
		this.hciPromise = this.init();
	}

	async init() {
		const hci = await this.bus.getProxyObject(
			'org.bluez',
			'/org/bluez/hci0'
		);

		// Create `BluetoothPlayer` instances for all registered bluetooth nodes
		// TODO: update `players` when new devices (nodes) are registered
		for (const node of hci.nodes) {
			const player = new BluetoothPlayer(this.bus, node);
			player.on('connect', () => this.onPlayerConnect(player));
			this.players.set(node, player);
		}

		return hci;
	}

	onPlayerConnect = (player: BluetoothPlayer) => {
		this.emit('connect', player);
	};
}

interface VariantMap<T = any> {
	[name: string]: Variant<T>;
}

export class BluetoothPlayer extends EventEmitter {
	bus: MessageBus;
	node: string;
	name: string;
	connected: boolean;
	proxyObjectPromise: Promise<ProxyObject>;
	fdPromise: Promise<ProxyObject> | null;
	playerPromise: Promise<ProxyObject> | null;
	propertyListeningNodes: Set<string>;

	constructor(bus: MessageBus, node: string) {
		super();
		debug('Creating BluetoothPlayer instance for %o', node);
		this.bus = bus;
		this.node = node;
		this.name = '';
		this.connected = false;
		this.on('connect', this.onConnect);
		this.proxyObjectPromise = this.initProxyObject();
		this.fdPromise = null;
		this.playerPromise = null;
		this.propertyListeningNodes = new Set();
	}

	async initProxyObject(ensureFdNode = false) {
		let obj: ProxyObject | null = null;
		while (!obj) {
			debug('getting proxy object');
			obj = await this.bus.getProxyObject('org.bluez', this.node);
			if (
				ensureFdNode &&
				!obj.nodes.some((n) => /^fd\d+$/.test(basename(n)))
			) {
				obj = null;
				await new Promise((r) => setTimeout(r, 100));
			}
		}
		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		if (!this.propertyListeningNodes.has(this.node)) {
			properties.on('PropertiesChanged', this.onPropertyChange);
			this.propertyListeningNodes.add(this.node);
		}
		const [name, connected] = (
			await Promise.all([
				properties.Get('org.bluez.Device1', 'Alias'),
				properties.Get('org.bluez.Device1', 'Connected'),
			])
		).map((v) => v.value);
		this.name = name;

		if (connected !== this.connected) {
			debug('Connected: %o %o', this.name || this.node, connected);
			this.connected = connected;
			this.emit(connected ? 'connect' : 'disconnect');
		}

		return obj;
	}

	async initFd() {
		debug('initFd');
		const obj = await this.proxyObjectPromise;
		const fdNode = obj.nodes.find((node) => /^fd\d+$/.test(basename(node)));
		if (!fdNode) {
			throw new Error('Could not determine "fd" node');
		}
		debug('Using fd node: %o', fdNode);
		const fd = await obj.bus.getProxyObject('org.bluez', fdNode);

		if (!this.propertyListeningNodes.has(fdNode)) {
			const properties = fd.getInterface(
				'org.freedesktop.DBus.Properties'
			);
			properties.on('PropertiesChanged', this.onFdPropertyChange);
			this.propertyListeningNodes.add(fdNode);
		}

		return fd;
	}

	async initPlayer(playerNode: string) {
		debug('initPlayer %o', playerNode);
		const obj = await this.proxyObjectPromise;
		const player = await obj.bus.getProxyObject('org.bluez', playerNode);

		if (!this.propertyListeningNodes.has(playerNode)) {
			const properties = player.getInterface(
				'org.freedesktop.DBus.Properties'
			);
			properties.on('PropertiesChanged', this.onPlayerPropertyChange);
			this.propertyListeningNodes.add(playerNode);
		}

		return player;
	}

	async getPlayerNode(): Promise<string | null> {
		const obj = await this.proxyObjectPromise;
		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		try {
			const player = await properties.Get(
				'org.bluez.MediaControl1',
				'Player'
			);
			return player.value;
		} catch (err) {
			debug('getPlayerNode() error: %o', err);
			return null;
		}
	}

	private onConnect = () => {
		// New "nodes" may have been set for fd/player upon connection,
		// so regenerate the original proxy object
		this.proxyObjectPromise = this.initProxyObject(true);

		this.fdPromise = this.initFd();
		this.getPlayerNode().then((playerNode) => {
			if (playerNode) {
				this.playerPromise = this.initPlayer(playerNode);
			}
		});
	};

	private onPropertyChange = (iface: string, changed: any) => {
		console.log('onPropertyChange', this.name || this.node, iface, changed);
		if (changed.Connected) {
			const connected = changed.Connected.value;
			if (connected !== this.connected) {
				debug('Connected: %o %o', this.name || this.node, connected);
				this.connected = connected;
				this.emit(connected ? 'connect' : 'disconnect');
			}
		}
		if (changed.Player) {
			const node = changed.Player.value;
			debug('Setting player: %o', node);
			this.playerPromise = this.initPlayer(node);
		}
	};

	private onFdPropertyChange = (iface: string, changed: VariantMap) => {
		if (changed.Volume) {
			this.emit('volume', changed.Volume.value / 127);
		}
	};

	private onPlayerPropertyChange = (iface: string, changed: VariantMap) => {
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
		state.updatedAt = Date.now();
		this.emit('update', state);
	};

	async getVolume(): Promise<number> {
		const fd = await this.fdPromise;
		if (!fd) {
			throw new Error('no fd');
		}
		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		const volume = await properties.Get(
			'org.bluez.MediaTransport1',
			'Volume'
		);
		return volume.value / 127;
	}

	async setVolume(volume: number) {
		const fd = await this.fdPromise;
		if (!fd) {
			throw new Error('no fd');
		}
		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		const v = new Variant('q', Math.round(volume * 127));
		await properties.Set('org.bluez.MediaTransport1', 'Volume', v);
	}

	async play() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Play();
	}

	async pause() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Pause();
	}

	async stop() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Stop();
	}

	async next() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Next();
	}

	async previous() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Previous();
	}

	async fastForward() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.FastForward();
	}

	async rewind() {
		const player = await this.playerPromise;
		if (!player) {
			throw new Error('no player');
		}
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Rewind();
	}
}
