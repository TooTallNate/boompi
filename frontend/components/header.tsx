// CSS
import styles from '@styles/header.module.css';

// Components
import Clock from '@components/clock';

// Icons
import Battery from '@components/icons/battery';
import Bluetooth from '@components/icons/bluetooth';
import Mobile from '@components/icons/mobile';
import Volume from '@components/icons/volume';

interface HeaderProps {
	now: Date;
	bluetoothName: string | null;
	isCharging: boolean;
	battery: number;
	volume: number;
}

export default function Header({
	now,
	bluetoothName,
	isCharging,
	battery,
	volume,
}: HeaderProps) {
	const isConnected = typeof bluetoothName === 'string';
	const volumeLevel = volume === 0 ? 0 : Math.floor(volume * 3) + 1;
	return (
		<section className={styles.header}>
			<div className={styles.left}>
				<Clock date={now} />
			</div>
			<div className={styles.center}>
				<Mobile />
				{bluetoothName}
			</div>
			<div className={styles.right}>
				<Volume level={volumeLevel} />
				<Bluetooth
					isConnected={isConnected}
					className={styles.bluetooth}
				/>
				<Battery
					percentage={battery}
					charging={isCharging}
					chargeClassName={styles.battCharge}
					fillClassName={styles.battFill}
				/>
			</div>
		</section>
	);
}
