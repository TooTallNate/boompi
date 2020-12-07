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
				<strong>{battery.current.toFixed(2)}</strong> Milliamps
			</>
		) : (
			<>
				<strong>{(battery.current / 1000).toFixed(2)}</strong> Amps
			</>
		);
	return (
		<div>
			<div>
				<strong>{battery.voltage.toFixed(2)}</strong> Volts
			</div>
			<div>{amps}</div>
			<div>
				<strong>{(battery.power / 1000).toFixed(2)}</strong> Watts
			</div>
			<div>
				<strong>{`${(battery.percentage * 100).toFixed(1)}%`}</strong>{' '}
				Remaining
			</div>
		</div>
	);
}
