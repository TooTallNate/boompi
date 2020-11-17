// CSS
import styles from '@styles/index.module.css';

// Components
import Header from '@components/header';
import NowPlaying from '@components/now-playing';

// Hooks
import useNow from '@lib/use-now';
import useBackend from '@lib/use-backend';

export default function Index() {
	const { now } = useNow();
	const {
		artist,
		track,
		album,
		position,
		duration,
		volume,
		isPlaying,
		setVolume,
		setPosition,
		setPlay,
		setPause,
		setRewind,
		setFastForward,
	} = useBackend({
		url: 'ws://localhost:3001',
	});

	return (
		<main className={styles.main}>
			<Header now={now} volume={volume} />
			<section className={styles.content}>
				<NowPlaying
					artist={artist}
					track={track}
					album={album}
					volume={volume}
					position={position}
					duration={duration}
					isPlaying={isPlaying}
					onVolumeChange={setVolume}
					onPositionChange={setPosition}
					onPlay={setPlay}
					onPause={setPause}
					onRewind={setRewind}
					onFastForward={setFastForward}
				/>
			</section>
		</main>
	);
}