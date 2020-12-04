import createDebug from 'debug';
import { basename, join } from 'path';
import { camelCase } from 'camel-case';
import { EventEmitter, once } from 'events';
import { MessageBus, ProxyObject, Variant } from 'dbus-next';

const debug = createDebug('boompi:backend:bluetooth');

export class Bluetooth extends EventEmitter {
	bus: MessageBus;
	hciPromise: Promise<ProxyObject>;
	players: Map<string, BluetoothPlayer2>;

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
			const player = new BluetoothPlayer2(this.bus, node);
			player.on('connect', () => this.onPlayerConnect(player));
			this.players.set(node, player);
		}

		return hci;
	}

	onPlayerConnect = (player: BluetoothPlayer2) => {
		this.emit('connect', player);
	};
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

	properties.on('PropertiesChanged', (iface: string, changed: any) => {
		console.log('org.bluez PropertiesChanged', iface, changed);
	});

	return new BluetoothPlayer(obj, name);
}

interface VariantMap<T = any> {
	[name: string]: Variant<T>;
}

export class BluetoothPlayer2 extends EventEmitter {
	bus: MessageBus;
	node: string;
	name: string;
	connected: boolean;
	proxyObjectPromise: Promise<ProxyObject>;
	fdPromise: Promise<ProxyObject> | null;
	playerPromise: Promise<ProxyObject> | null;

	constructor(bus: MessageBus, node: string) {
		super();
		debug('Creating BluetoothPlayer2 instance for %o', node);
		this.bus = bus;
		this.node = node;
		this.name = '';
		this.connected = false;
		this.on('connect', this.onConnect);
		this.proxyObjectPromise = this.initProxyObject();
		this.fdPromise = null;
		this.playerPromise = null;
	}

	async initProxyObject(ensureFdNode = false) {
		let obj: ProxyObject | null = null;
		while (!obj) {
			debug('getting proxy object');
			obj = await this.bus.getProxyObject('org.bluez', this.node);
			console.log(this.node, obj.nodes);
			if (
				ensureFdNode &&
				!obj.nodes.some((n) => /^fd\d+$/.test(basename(n)))
			) {
				obj = null;
			}
		}
		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onPropertyChange);
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

		const properties = fd.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onFdPropertyChange);

		return fd;
	}

	async initPlayer(playerNode: string) {
		debug('initPlayer %o', playerNode);
		const obj = await this.proxyObjectPromise;
		const player = await obj.bus.getProxyObject('org.bluez', playerNode);

		const properties = player.getInterface(
			'org.freedesktop.DBus.Properties'
		);
		properties.on('PropertiesChanged', this.onPlayerPropertyChange);

		return player;
	}

	async getPlayerNode(): Promise<string | null> {
		const obj = await this.proxyObjectPromise;
		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		const player = await properties.Get(
			'org.bluez.MediaControl1',
			'Player'
		);
		return player.value;
	}

	private onConnect = () => {
		// New "nodes" may have been set for fd/player upon connection,
		// so regenerate the original proxy object
		//this.proxyObjectPromise = new Promise(r => setTimeout(r, 1000)).then(() => this.initProxyObject());
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
			this.initPlayer(node);
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

export class BluetoothPlayer extends EventEmitter {
	obj: ProxyObject;
	name: string;
	fdPromise: Promise<ProxyObject>;
	playerPromise: Promise<ProxyObject>;

	constructor(obj: ProxyObject, name: string) {
		super();
		this.obj = obj;
		this.name = name;
		console.log(obj);

		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onPropertyChange);

		const fdNode = obj.nodes.find((node) => /^fd\d+$/.test(basename(node)));
		if (!fdNode) {
			throw new Error('Could not determine "fd" node');
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
			const { value } = changed[prop];
			if (prop === 'Player') {
				debug('Setting player: %o', value);
				this.playerPromise = this.obj.bus.getProxyObject(
					'org.bluez',
					value
				);
				this.initPlayer();
			} else if (prop === 'Connected') {
				if (value === false) {
					debug(
						'Bluetooth player disconnected: %s (%s)',
						this.obj.path,
						this.name
					);
					this.emit('disconnect');
				}
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
			bluetoothName: this.name,
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
