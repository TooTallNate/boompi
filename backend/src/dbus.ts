import dbus from 'dbus-next';
import { getBluetoothPlayer, BluetoothPlayer, BluetoothManager } from './bluetooth';

async function main() {
	const bus = dbus.systemBus();
	const bt = new BluetoothManager(bus);
	bt.on('connect', (player: BluetoothPlayer) => {
		console.log('"connect" %s', player.name);
	});
	bt.on('disconnect', (player: BluetoothPlayer) => {
		console.log('"disconnect" %s', player.name);
	});
}

async function main2() {
	const bus = dbus.systemBus();
	let obj = await bus.getProxyObject(
		'org.freedesktop.DBus',
		'/org/freedesktop/DBus'
	);
	let iface = obj.getInterface('org.freedesktop.DBus');

	let names = await iface.ListNames();
	console.log({names});


	obj = await bus.getProxyObject('org.bluez', '/org/bluez/hci0');
	console.log({ obj });

	let properties = obj.getInterface('org.freedesktop.DBus.Properties');
	console.log({ properties });

	properties.on('PropertiesChanged', (iface: string, changed: any) => {
		console.log('org.bluez PropertiesChanged', iface, changed);
	});

	let props = await properties.GetAll('org.bluez.Media1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.Adapter1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.NetworkServer1');
	console.log({ props });

	obj = await bus.getProxyObject(
		'org.bluez',
		'/org/bluez/hci0/dev_AC_88_FD_22_2F_ED'
	);
	console.log({ obj });

	properties = obj.getInterface('org.freedesktop.DBus.Properties');
	//console.log({ properties });
	properties.on('PropertiesChanged', (iface: string, changed: any) => {
		console.log('org.bluez PropertiesChanged', iface, changed);
	});

	props = await properties.GetAll('org.bluez.Device1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.Network1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.MediaControl1');
	console.log({ props });
return;

	obj = await bus.getProxyObject(
		'org.bluez',
		'/org/bluez/hci0/dev_AC_88_FD_22_2F_ED/fd2'
	);
	console.log({ obj });

	properties = obj.getInterface('org.freedesktop.DBus.Properties');
	//console.log({ properties });
	props = await properties.GetAll('org.bluez.MediaTransport1');
	console.log({ props });

	properties.on(
		'PropertiesChanged',
		(iface: any, changed: any, invalidated: any) => {
			console.log({ iface, changed, invalidated });
			for (let prop of Object.keys(changed)) {
				console.log(`property changed: ${prop}`);
			}
		}
	);

	obj = await bus.getProxyObject(
		'org.bluez',
		'/org/bluez/hci0/dev_AC_88_FD_22_2F_ED/player0'
	);
	//console.log({ obj });

	properties = obj.getInterface('org.freedesktop.DBus.Properties');
	//console.log({ properties });
	props = await properties.GetAll('org.bluez.MediaPlayer1');
	console.log({ props });

	const track = await properties.Get('org.bluez.MediaPlayer1', 'Track');
	console.log({ track });

	properties.on(
		'PropertiesChanged',
		(iface: any, changed: any, invalidated: any) => {
			console.log({ iface, changed, invalidated });
			for (let prop of Object.keys(changed)) {
				console.log(`property changed: ${prop}`);
			}
		}
	);

	const mp = obj.getInterface('org.bluez.MediaPlayer1');
	console.log(mp);
	await mp.Stop();
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
