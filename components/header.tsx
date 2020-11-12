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
}

export default function Header({ now }: HeaderProps) {
	return (
		<section className={styles.header}>
			<div className={styles.right}>
				<Volume level={3} />
				<Bluetooth />
				<Battery percentage={0.2} charging />
			</div>
			<div className={styles.left}>
				<Clock date={now} />
			</div>
		</section>
	);
}
