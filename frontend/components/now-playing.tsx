import { useCallback } from 'react';

// CSS
import styles from '@styles/now-playing.module.css';

// Icons
import Play from '@components/icons/play';
import Rewind from '@components/icons/rewind';
import Volume from '@components/icons/volume';
import FastForward from '@components/icons/fast-forward';

interface NowPlayingProps {
	artist: string;
	track: string;
	album: string;
	position: number;
	duration: number;
	volume: number;
	onPlay: () => void;
	onPause: () => void;
	onRewind: () => void;
	onFastForward: () => void;
	onVolumeChange: (value: number) => void;
	onPositionChange: (value: number) => void;
}

export default function NowPlaying({
	artist,
	track,
	album,
	volume,
	onPlay,
	onPause,
	onRewind,
	onFastForward,
	onVolumeChange,
	onPositionChange,
}: NowPlayingProps) {
	const onVolume = useCallback(
		(event) => {
			onVolumeChange(event.currentTarget.value / 100);
		},
		[onVolumeChange]
	);
	const onVolumeMute = useCallback(
		(event) => {
			onVolumeChange(0);
		},
		[onVolumeChange]
	);
	const onVolumeMax = useCallback(
		(event) => {
			onVolumeChange(1);
		},
		[onVolumeChange]
	);
	return (
		<div className={styles.nowPlaying}>
			<div className={styles.artist}>{artist}</div>
			<div className={styles.track}>{track}</div>
			<div className={styles.album}>{album}</div>
			<div className={styles.position}>
				1:36
				<input type="range" min="0" max="100" />
				-2:14
			</div>
			<div className={styles.controls}>
				<Rewind onClick={onRewind} />
				<Play onClick={onPlay} />
				<FastForward onClick={onFastForward} />
			</div>
			<div className={styles.volume}>
				<Volume onClick={onVolumeMute} />
				<input
					type="range"
					min="0"
					max="100"
					onInput={onVolume}
					value={volume * 100}
				/>
				<Volume level={3} onClick={onVolumeMax} />
			</div>
		</div>
	);
}
