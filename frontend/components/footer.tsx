import { Battery } from '@lib/types';

// CSS
import styles from '@styles/footer.module.css';

// Components
import Clock from '@components/clock';

// Icons
import BatteryIcon from '@components/icons/battery';
import Bluetooth from '@components/icons/bluetooth';
import Mobile from '@components/icons/mobile';
import Volume from '@components/icons/volume';

// Hooks
import useNow from '@lib/use-now';

interface FooterProps {
	bluetoothName: string | null;
	battery: Battery | null;
	volume: number | null;
	onBatteryClick: () => void;
}

const daysOfWeek = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];

export default function Footer({
	bluetoothName,
	battery,
	volume,
	onBatteryClick,
}: FooterProps) {
	const { now } = useNow();
	const isConnected = typeof bluetoothName === 'string';

	let volumeIcon = null;
	if (typeof volume === 'number') {
		const volumeLevel = volume === 0 ? 0 : Math.floor(volume * 3) + 1;
		volumeIcon = <Volume level={volumeLevel} />;
	}

	let batteryIcon = null;
	if (battery) {
		const batteryClasses = [styles.battery];
		if (battery.percentage < 0.2) {
			batteryClasses.push(styles.low);
		}
		let isCharging = battery.current <= -20;
		if (isCharging || battery.percentage >= 0.99) {
			batteryClasses.push(styles.charging);
		}
		batteryIcon = (
			<BatteryIcon
				className={batteryClasses.join(' ')}
				percentage={battery.percentage}
				isCharging={isCharging}
				onClick={onBatteryClick}
			/>
		);
	}

	return (
		<section className={styles.footer}>
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
				{batteryIcon}
			</div>
		</section>
	);
}
