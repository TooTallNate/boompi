// CSS
import styles from '@styles/index.module.css';

// Components
import Header from '@components/header';
import NowPlaying from '@components/now-playing';

// Hooks
import useNow from '@lib/use-now';

export default function Index() {
	const { now } = useNow();

	return (
		<main className={styles.main}>
			<Header now={now} />
			<section className={styles.content}>
				<NowPlaying />
			</section>
		</main>
	);
}
