import { useEffect, useState } from 'react';

// CSS
import styles from '@styles/now-playing.module.css';

// Components
import Marquee from '@components/marquee';
import TrackPosition from '@components/track-position';

// Icons
import { Play, Pause, TrackNext, TrackPrevious } from '@components/icons/radix';
import VolumeControl from './volume-control';

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
			<TrackPosition playPosition={playPosition} duration={duration} />
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
				<VolumeControl
					volume={volume}
					onVolumeChange={onVolumeChange}
				/>
			)}
		</div>
	);
}
