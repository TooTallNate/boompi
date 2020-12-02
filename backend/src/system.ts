import execa from 'execa';
import createDebug from 'debug';

const debug = createDebug('boompi:backend:system');

export async function setVolume(volume: number): Promise<void> {
	const percent = `${Math.round(volume * 100)}%`;
	debug('Setting system volume to %o', percent);
	await execa('pactl', ['set-sink-volume', '@DEFAULT_SINK@', percent]);
}
