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
	isCharging: boolean;
	battery: Battery | null;
	volume: number | null;
	onBatteryClick: () => void;
}

export default function Header({
	bluetoothName,
	isCharging,
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
	return (
		<section className={styles.header}>
			<div className={styles.left}>
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
						className={styles.battery}
						percentage={battery.percentage}
						isCharging={isCharging}
						onClick={onBatteryClick}
					/>
				)}
			</div>
		</section>
	);
}
