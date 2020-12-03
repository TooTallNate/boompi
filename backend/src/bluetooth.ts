import createDebug from 'debug';
import { basename, join } from 'path';
import { camelCase } from 'camel-case';
import { EventEmitter, once } from 'events';
import { MessageBus, ProxyObject, Variant } from 'dbus-next';

const debug = createDebug('boompi:backend:bluetooth');

export async function* getBluetoothPlayerEvents(bus: MessageBus) {
	const hci = await bus.getProxyObject('org.bluez', '/org/bluez/hci0');

	// TODO: update `players` when new devices (nodes) are connected
	const players = hci.nodes.map((node) => new BluetoothPlayer2(bus, node));
	//console.log(players);

	while (true) {
		const player = await Promise.race(
			players.map((p) => p.waitForConnect())
		);
		console.log('player connected', player.name);
		yield { connect: true, player };

		await once(player, 'disconnect');
		console.log('player disconnected', player.name);
		yield { disconnect: true, player };
	}
}

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
		this.proxyObjectPromise = this.initProxyObject();
		this.fdPromise = null;
		this.playerPromise = null;
	}

	async initProxyObject() {
		const obj = await this.bus.getProxyObject('org.bluez', this.node);
		//console.log(node, obj.nodes);
		const properties = obj.getInterface('org.freedesktop.DBus.Properties');
		properties.on('PropertiesChanged', this.onPropertyChange);
		const [name, connected] = (
			await Promise.all([
				properties.Get('org.bluez.Device1', 'Alias'),
				properties.Get('org.bluez.Device1', 'Connected'),
			])
		).map((v) => v.value);
		this.name = name;
		this.connected = connected;
		this.emit('init');
		return obj;
	}

	async initFd() {}

	async initPlayer() {}

	onPropertyChange = (iface: string, changed: any) => {
		console.log('onPropertyChange', this.name || this.node, iface, changed);
		if (changed.Connected) {
			const connected = changed.Connected.value;
			if (connected !== this.connected) {
				debug('Connected: %o %o', this.name || this.node, connected);
				this.connected = connected;
				this.emit(connected ? 'connect' : 'disconnect');
			}
		}
		/*
		if (changed.Player) {
			const node = changed.Player.value;
			debug('Setting player: %o', node);
			this.playerPromise = this.obj.bus.getProxyObject(
				'org.bluez',
				node
			);
			this.initPlayer();
		}
		*/
	};

	async waitForConnect() {
		await this.proxyObjectPromise;
		if (!this.connected) {
			await once(this, 'connect');
		}
		return this;
	}

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
		return volume.value;
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
