import { Battery } from '@lib/types';
import { useEffect } from 'react';

// CSS
import styles from '@styles/battery.module.css';

// Components
import BatteryChart from '@components/battery-chart';

interface BatteryProps {
	battery: Battery;
	batteryFastPoll: (enabled: boolean) => void;
}

export default function BatteryPanel({
	battery,
	batteryFastPoll,
}: BatteryProps) {
	// Tell the backend to poll the battery status quickly
	useEffect(() => {
		batteryFastPoll(true);
		return () => {
			batteryFastPoll(false);
		};
	}, []);

	const amps =
		battery.current < 1000 && battery.current > -1000 ? (
			<>
				<td>{battery.current.toFixed(2)}</td>
				<td>Milliamps</td>
			</>
		) : (
			<>
				<td>{(battery.current / 1000).toFixed(2)}</td>
				<td>Amps</td>
			</>
		);
	return (
		<div className={styles.outer}>
			<div className={styles.chartContainer}>
				<BatteryChart battery={battery} />
			</div>
			<div className={styles.infoContainer}>
				<table className={styles.info}>
					<tbody>
						<tr>
							<td>{battery.voltage.toFixed(2)}</td>
							<td>Volts</td>
							<td>{(battery.power / 1000).toFixed(2)}</td>
							<td>Watts</td>
						</tr>
						<tr>
							{amps}
							<td>
								{(battery.percentage * 100).toFixed(1)}
								<span className={styles.percent}>%</span>
							</td>
							<td>Remaining</td>
						</tr>
					</tbody>
				</table>
			</div>
		</div>
	);
}
