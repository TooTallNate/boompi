import fs from 'fs-extra';
import execa from 'execa';
import dotenv from 'dotenv';
import createDebug from 'debug';

const debug = createDebug('boompi:backend:system');

export async function setVolume(volume: number): Promise<void> {
	const percent = `${Math.round(volume * 100)}%`;
	debug('Setting system volume to %o', percent);

	try {
		// Set PulseAudio volume
		//await execa('pactl', ['set-sink-volume', '@DEFAULT_SINK@', percent]);

		// Set ALSA volume
		await execa('amixer', ['cset', 'numid=1', '--', percent]);
	} catch (err) {
		console.log(`Error while setting volume: ${err}`);
	}
}

export async function getPrettyHostname(): Promise<string | undefined> {
	const machineInfo = dotenv.parse(
		await fs.readFile("/etc/machine-info", "utf8")
	);
	return machineInfo.PRETTY_HOSTNAME;
}