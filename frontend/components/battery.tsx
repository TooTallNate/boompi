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
}

export default function BatteryPanel({ battery }: BatteryProps) {
	const history = useRef<Map<number, Battery>>(new Map());
	if (!history.current.has(battery.date)) {
		history.current.set(battery.date, battery);
	}
	useEffect(() => {
		debug('batt panel start');
		return () => {
			debug('batt panel end');
		};
	}, []);
	console.log(history.current);
	return JSON.stringify(history.current);
}
