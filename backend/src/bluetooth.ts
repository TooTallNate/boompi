import { basename, join } from 'path';
import createDebug from 'debug';
import { EventEmitter } from 'events';
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
	console.log({ props });
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
		properties.on('PropertiesChanged', this.onPropertyChange.bind(this));

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
		properties.on('PropertiesChanged', this.onFdPropertyChange.bind(this));
	}

	async initPlayer() {
		const player = await this.playerPromise;
		const properties = player.getInterface(
			'org.freedesktop.DBus.Properties'
		);

		const props = await properties.GetAll('org.bluez.MediaPlayer1');
		console.log({ props });

		properties.on(
			'PropertiesChanged',
			this.onPlayerPropertyChange.bind(this)
		);
	}

	onFdPropertyChange(iface: string, changed: VariantMap, invalidated: any[]) {
		for (const prop of Object.keys(changed)) {
			if (prop === 'Volume') {
				this.emit('volume', changed[prop].value / 127);
			}
		}
	}

	onPlayerPropertyChange(
		iface: string,
		changed: VariantMap,
		invalidated: any[]
	) {
		console.log('onPlayerPropertyChanged', iface, changed);
	}

	onPropertyChange(iface: string, changed: VariantMap, invalidated: any[]) {
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
	}

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

	async stop() {
		const player = await this.playerPromise;
		const mediaPlayer = player.getInterface('org.bluez.MediaPlayer1');
		await mediaPlayer.Stop();
	}
}
