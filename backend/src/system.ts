import execa, { ExecaChildProcess } from 'execa';

export async function getVolume(): Promise<number> {
	return 0;
}

export async function setVolume(volume: number): Promise<void> {
	const percent = Math.round(volume * 100);
	await execa('pactl', ['set-sink-volume', '@DEFAULT_SINK@', `${percent}%`]);
}
