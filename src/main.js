//require('util').inspect.defaultOptions.depth = 10;
const dbus = require('dbus-next');
const bus = dbus.systemBus();

async function main() {
	let obj;

	obj = await bus.getProxyObject(
		'org.freedesktop.DBus',
		'/org/freedesktop/DBus'
	);
	let iface = obj.getInterface('org.freedesktop.DBus');
	//console.log(await iface.GetId());
	//return;
	//let names = await iface.ListNames();
	//console.log(names);

	obj = await bus.getProxyObject(
		'org.bluez',
		'/org/bluez/hci0/dev_AC_88_FD_22_2F_ED'
	);
	console.log({ obj });

	let properties = obj.getInterface('org.freedesktop.DBus.Properties');
	//console.log({ properties });
	let props = await properties.GetAll('org.bluez.Device1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.Network1');
	console.log({ props });

	props = await properties.GetAll('org.bluez.MediaControl1');
	console.log({ props });

	obj = await bus.getProxyObject(
		'org.bluez',
		'/org/bluez/hci0/dev_AC_88_FD_22_2F_ED/fd0'
	);
	console.log({ obj });

	properties = obj.getInterface('org.freedesktop.DBus.Properties');
	//console.log({ properties });
	props = await properties.GetAll('org.bluez.MediaTransport1');
	console.log({ props });

	properties.on('PropertiesChanged', (iface, changed, invalidated) => {
		console.log({ iface, changed, invalidated });
		for (let prop of Object.keys(changed)) {
			console.log(`property changed: ${prop}`);
		}
	});

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

	properties.on('PropertiesChanged', (iface, changed, invalidated) => {
		console.log({ iface, changed, invalidated });
		for (let prop of Object.keys(changed)) {
			console.log(`property changed: ${prop}`);
		}
	});

	const mp = obj.getInterface('org.bluez.MediaPlayer1');
	console.log(mp);
	await mp.Stop();
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
