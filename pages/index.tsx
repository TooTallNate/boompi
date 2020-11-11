// Dependencies
import { useState } from 'react';

// CSS
import styles from '@styles/index.module.css';

// Components
import Clock from '@components/clock';

// Icons
import Battery from '@components/icons/battery';
import Bluetooth from '@components/icons/bluetooth';
import Close from '@components/icons/close';
import FastForward from '@components/icons/fast-forward';
import Next from '@components/icons/next';
import Pause from '@components/icons/pause';
import Play from '@components/icons/play';
import Previous from '@components/icons/previous';
import Rewind from '@components/icons/rewind';
import SkipBack from '@components/icons/skip-back';
import SkipForward from '@components/icons/skip-forward';
import Volume from '@components/icons/volume';

// Hooks
import useNow from '@lib/use-now';

export default function Index() {
	const { now } = useNow();

	return (
		<main className={styles.main}>
			<Clock date={now} />
			<Play />
			<Pause />

			<Rewind />
			<FastForward />

			<Previous />
			<Next />

			<SkipBack />
			<SkipForward />

			<Volume level={3} />
			<Battery percentage={0.2} charging />
			<Bluetooth />
			<Close />
		</main>
	);
}
