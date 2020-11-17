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
	const { volume, setVolume } = useBackend({ url: 'ws://localhost:3001' });

	return (
		<main className={styles.main}>
			<Header now={now} volume={volume} />
			<section className={styles.content}>
				<NowPlaying volume={volume} onVolumeChange={setVolume} />
			</section>
		</main>
	);
}
