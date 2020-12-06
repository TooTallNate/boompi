import execa from 'execa';
import createDebug from 'debug';

const debug = createDebug('boompi:backend:system');

export async function setVolume(volume: number): Promise<void> {
	const percent = `${Math.round(volume * 100)}%`;
	debug('Setting system volume to %o', percent);

	// Stopped working after a system update for some reason :(
	//await execa('pactl', ['set-sink-volume', '@DEFAULT_SINK@', percent]);

	await execa('amixer', ['cset', 'numid=1', '--', percent]);
}
