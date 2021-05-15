import { useCallback, useEffect, useState } from 'react';

// CSS
import styles from '@styles/now-playing.module.css';

// Components
import Marquee from '@components/marquee';

// Icons
import Volume from '@components/icons/volume';
import { Play, Pause, TrackNext, TrackPrevious } from '@components/icons/radix';

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
	const [playPosition, setPlayPosition] = useState(
		position + (isPlaying ? Date.now() - positionChangedAt : 0)
	);

	const onVolume = useCallback(
		(event) => {
			const vol = parseInt(event.currentTarget.value, 10);
			onVolumeChange(vol / 100);
		},
		[onVolumeChange]
	);

	useEffect(() => {
		setPlayPosition(
			position + (isPlaying ? Date.now() - positionChangedAt : 0)
		);
	}, [position, positionChangedAt, isPlaying]);

	// Update the artificial play position at 5 FPS
	useEffect(() => {
		if (!isPlaying) return;
		const newPlayPosition = position + (Date.now() - positionChangedAt);
		if (newPlayPosition >= duration) return;
		const t = setTimeout(() => {
			if (!isPlaying) return;
			const newPlayPosition = position + (Date.now() - positionChangedAt);
			if (newPlayPosition <= duration) {
				setPlayPosition(newPlayPosition);
			}
		}, 200);
		return () => clearTimeout(t);
	}, [isPlaying, position, positionChangedAt, duration, playPosition]);

	return (
		<div className={styles.nowPlaying}>
			<div className={styles.artist}>
				<Marquee>{artist}</Marquee>
			</div>
			<div className={styles.track}>
				<Marquee>{track}</Marquee>
			</div>
			<div className={styles.album}>
				<Marquee>{album}</Marquee>
			</div>
			<div className={styles.position}>
				<div className={styles.label}>
					{formatSeconds(playPosition)}
				</div>
				<input
					type="range"
					min={0}
					max={duration}
					value={playPosition}
					readOnly
				/>
				<div className={styles.label}>
					-{formatSeconds(duration - playPosition)}
				</div>
			</div>
			<div className={styles.controls}>
				<TrackPrevious onClick={onRewind} />
				{isPlaying ? (
					<Pause onClick={onPause} />
				) : (
					<Play onClick={onPlay} />
				)}
				<TrackNext onClick={onFastForward} />
			</div>
			{typeof volume === 'number' && (
				<div className={styles.volume}>
					<div className={styles.label}>
						<Volume level={1} className={styles.volumeMin} />
					</div>
					<input
						type="range"
						min="0"
						max="100"
						onInput={onVolume}
						value={volume * 100}
					/>
					<div className={styles.label}>
						<Volume level={3} />
					</div>
				</div>
			)}
		</div>
	);
}
