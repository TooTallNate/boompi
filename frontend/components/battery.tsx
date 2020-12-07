import createDebug from 'debug';
import { Battery } from '@lib/types';
import { useEffect, useRef } from 'react';
import {
	LineChart,
	ReferenceLine,
	ResponsiveContainer,
	XAxis,
	YAxis,
	AxisDomain,
	Tooltip,
	Line,
} from 'recharts';

// CSS
import styles from '@styles/battery.module.css';

const debug = createDebug('boompi:components:battery');

interface BatteryProps {
	battery: Battery | null;
	batteryFastPoll: (enabled: boolean) => void;
}

export default function BatteryPanel({
	battery,
	batteryFastPoll,
}: BatteryProps) {
	const history = useRef<Map<number, Battery>>(new Map());
	if (!history.current.has(battery.date)) {
		history.current.set(battery.date, battery);
	}

	// Tell the backend to poll the battery status quickly
	useEffect(() => {
		debug('batt panel start');
		batteryFastPoll(true);
		return () => {
			debug('batt panel end');
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
		<table className={styles.info}>
			<tbody>
				<tr>
					<td>{battery.voltage.toFixed(2)}</td>
					<td>Volts</td>
				</tr>
				<tr>{amps}</tr>
				<tr>
					<td>{(battery.power / 1000).toFixed(2)}</td>
					<td>Watts</td>
				</tr>
				<tr>
					<td>{(battery.percentage * 100).toFixed(1)}<span className={styles.percent}>%</span></td>
					<td>Remaining</td>
				</tr>
			</tbody>
		</table>
	);
}
