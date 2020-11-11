import { useState } from 'react';

import styles from '@styles/index.module.css';

import Bluetooth from '@components/icons/bluetooth';
import Close from '@components/icons/close';
import Pause from '@components/icons/pause';
import Play from '@components/icons/play';
import Previous from '@components/icons/previous';
import Volume from '@components/icons/volume';

export default function Index() {
	return <main className={styles.main}>
		hello
		<Close />
		<Previous />
		<Pause />
		<Play />
		<Volume level={2} />
		<Bluetooth />
	</main>;
}
