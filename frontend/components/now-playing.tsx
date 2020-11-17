import { useCallback } from 'react';

// CSS
import styles from '@styles/now-playing.module.css';

// Icons
import Play from '@components/icons/play';
import Pause from '@components/icons/pause';
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
	isPlaying: boolean;
	onPlay: () => void;
	onPause: () => void;
	onRewind: () => void;
	onFastForward: () => void;
	onVolumeChange: (value: number) => void;
	onPositionChange: (value: number) => void;
}

function formatSeconds(ms: number) {
	const minutes = Math.floor(ms / (1000 * 60));
	const seconds = Math.floor((ms - minutes * (1000 * 60)) / 1000);
	return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

export default function NowPlaying({
	artist,
	track,
	album,
	position,
	duration,
	volume,
	isPlaying,
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
	const onPosition = useCallback(
		(event) => {
			const { value } = event.currentTarget;
			onPositionChange(parseInt(value, 10));
		},
		[onPositionChange, duration]
	);
	return (
		<div className={styles.nowPlaying}>
			<div className={styles.artist}>{artist}</div>
			<div className={styles.track}>{track}</div>
			<div className={styles.album}>{album}</div>
			<div className={styles.position}>
				{formatSeconds(position)}
				<input
					type="range"
					min="0"
					max={duration}
					onChange={onPosition}
					value={position}
				/>
				-{formatSeconds(duration - position)}
			</div>
			<div className={styles.controls}>
				<Rewind onClick={onRewind} />
				{isPlaying ? (
					<Pause onClick={onPause} />
				) : (
					<Play onClick={onPlay} />
				)}
				<FastForward onClick={onFastForward} />
			</div>
			<div className={styles.volume}>
				<Volume onClick={onVolumeMute} />
				<input
					type="range"
					min="0"
					max="100"
					onChange={onVolume}
					value={volume * 100}
				/>
				<Volume level={3} onClick={onVolumeMax} />
			</div>
		</div>
	);
}
