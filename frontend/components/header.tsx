import { Battery } from '@lib/types';

// CSS
import styles from '@styles/header.module.css';

// Components
import Clock from '@components/clock';

// Icons
import BatteryIcon from '@components/icons/battery';
import Bluetooth from '@components/icons/bluetooth';
import Mobile from '@components/icons/mobile';
import Volume from '@components/icons/volume';

// Hooks
import useNow from '@lib/use-now';

interface HeaderProps {
	bluetoothName: string | null;
	battery: Battery | null;
	volume: number | null;
	onBatteryClick: () => void;
}

const daysOfWeek = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

export default function Header({
	bluetoothName,
	battery,
	volume,
	onBatteryClick,
}: HeaderProps) {
	const { now } = useNow();
	const isConnected = typeof bluetoothName === 'string';
	let volumeIcon = null;
	if (typeof volume === 'number') {
		const volumeLevel = volume === 0 ? 0 : Math.floor(volume * 3) + 1;
		volumeIcon = <Volume level={volumeLevel} />;
	}
	const batteryClasses = [styles.battery];
	if (battery && battery.percentage < 0.2) {
		batteryClasses.push(styles.low);
	}
	return (
		<section className={styles.header}>
			<div className={styles.left}>
				<span className={styles.dayOfWeek}>
					{daysOfWeek[now.getDay()]}
				</span>
				<Clock date={now} />
			</div>
			<div className={styles.center}>
				{typeof bluetoothName === 'string' && <Mobile />}
				{bluetoothName || '\u00A0'}
			</div>
			<div className={styles.right}>
				{volumeIcon}
				<Bluetooth
					className={styles.bluetooth}
					isConnected={isConnected}
				/>
				{battery && (
					<BatteryIcon
						className={batteryClasses.join(' ')}
						percentage={battery.percentage}
						isCharging={battery.current <= -100}
						onClick={onBatteryClick}
					/>
				)}
			</div>
		</section>
	);
}
