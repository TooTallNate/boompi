import { useState } from 'react';

import styles from '@styles/index.module.css';

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

export default function Index() {
	return <main className={styles.main}>
		<Bluetooth />
		<Close />

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
	</main>;
}
