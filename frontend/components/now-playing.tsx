import { useCallback, useEffect, useState } from 'react';

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
	positionChangedAt: number;
	duration: number;
	volume: number | null;
	isPlaying: boolean;
	onPlay: () => void;
	onPause: () => void;
	onRewind: () => void;
	onFastForward: () => void;
	onVolumeChange: (value: number) => void;
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
	positionChangedAt,
	duration,
	volume,
	isPlaying,
	onPlay,
	onPause,
	onRewind,
	onFastForward,
	onVolumeChange,
}: NowPlayingProps) {
	const [playPosition, setPlayPosition] = useState(position);

	const onVolume = useCallback(
		(event) => {
			const vol = parseInt(event.currentTarget.value, 10);
			onVolumeChange(vol / 100);
		},
		[onVolumeChange]
	);

	useEffect(() => {
		setPlayPosition(position);
	}, [position]);

	useEffect(() => {
		if (!isPlaying) return;
		const newPlayPosition = position + (Date.now() - positionChangedAt);
		if (newPlayPosition >= duration) return;
		function step() {
			if (!isPlaying) return;
			const newPlayPosition = position + (Date.now() - positionChangedAt);
			if (newPlayPosition <= duration) {
				setPlayPosition(newPlayPosition);
			}
		}
		const raf = window.requestAnimationFrame(step);
		return () => {
			window.cancelAnimationFrame(raf);
		};
	}, [isPlaying, position, positionChangedAt, duration, playPosition]);

	return (
		<div className={styles.nowPlaying}>
			<div className={styles.artist}>{artist}</div>
			<div className={styles.track}>{track}</div>
			<div className={styles.album}>{album}</div>
			<div className={styles.position}>
				<div className={styles.time}>{formatSeconds(playPosition)}</div>
				<input
					type="range"
					min="0"
					max={duration}
					value={playPosition}
					readOnly
				/>
				<div className={styles.time}>-{formatSeconds(duration - playPosition)}</div>
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
			{typeof volume === 'number' && (
				<div className={styles.volume}>
					<div className={styles.time}><Volume className={styles.volumeMin}/></div>
					<input
						type="range"
						min="0"
						max="100"
						onInput={onVolume}
						value={volume * 100}
					/>
					<div className={styles.time}><Volume level={3} /></div>
				</div>
			)}
		</div>
	);
}
