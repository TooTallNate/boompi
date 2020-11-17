// CSS
import styles from '@styles/header.module.css';

// Components
import Clock from '@components/clock';

// Icons
import Battery from '@components/icons/battery';
import Bluetooth from '@components/icons/bluetooth';
import Volume from '@components/icons/volume';

interface HeaderProps {
	now: Date;
	volume: number;
}

export default function Header({ now, volume }: HeaderProps) {
	const volumeLevel = volume === 0 ? 0 : Math.floor(volume * 3) + 1;
	return (
		<section className={styles.header}>
			<div className={styles.right}>
				<Volume level={volumeLevel} />
				<Bluetooth className={styles.bluetooth} />
				<Battery percentage={0.2} charging />
			</div>
			<div className={styles.left}>
				<Clock date={now} />
			</div>
		</section>
	);
}
