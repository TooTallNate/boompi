import ms from 'ms';
import createDebug from 'debug';
import { useCallback, useRef, useState } from 'react';

import { Battery } from '@lib/types';
import useRequestAnimationFrame from '@lib/use-raf';

import {
	CartesianGrid,
	LineChart,
	ReferenceLine,
	ResponsiveContainer,
	Label,
	XAxis,
	YAxis,
	AxisDomain,
	Legend,
	Line,
} from 'recharts';

interface BatteryChartProps {
	battery: Battery;
}

const debug = createDebug('boompi:components:battery-chart');

export default function BatteryChart({ battery }: BatteryChartProps) {
	const [now, setNow] = useState(Date.now());

	useRequestAnimationFrame(() => {
		setNow(Date.now());
	}, [now]);

	const history = useRef<Map<number, Battery>>(new Map());
	if (!history.current.has(battery.date)) {
		for (const batt of history.current.values()) {
			if (now - batt.date > ms('1m') + 2000) {
				history.current.delete(batt.date);
			}
		}

		history.current.set(battery.date, battery);
	}

	const xDomain: [AxisDomain, AxisDomain] = [
		useCallback(() => {
			return now - ms('1m');
		}, [now]),
		useCallback(() => {
			return now;
		}, [now]),
	];

	const data = Array.from(history.current.values());

	return (
		<ResponsiveContainer height="100%" width="100%">
			<LineChart data={data} margin={{ left: -10, right: -10 }}>
				<XAxis
					height={40}
					dataKey="date"
					type="number"
					tickFormatter={(val) => {
						const diff = now - val;
						return `${ms(diff)} ago`;
					}}
					allowDataOverflow={true}
					tick={{ fontSize: 10 }}
					stroke="#aaa"
					domain={xDomain}
				></XAxis>
				<YAxis
					width={80}
					yAxisId="left"
					tick={{ fontSize: 10 }}
					stroke="#aaa"
					domain={[17, 26]}
				>
					<Label
						value="Voltage"
						angle={-90}
						position="outside"
						fill="white"
						fontSize={14}
					/>
				</YAxis>
				<YAxis
					width={80}
					yAxisId="right"
					orientation="right"
					tick={{ fontSize: 10 }}
					stroke="#aaa"
					domain={[50, 300]}
				>
					<Label
						value="Amperage"
						angle={90}
						position="outside"
						fill="white"
						fontSize={14}
					/>
				</YAxis>
				<Legend />
				<Line
					yAxisId="left"
					type="monotone"
					dataKey="voltage"
					name="Volts"
					stroke="orange"
					isAnimationActive={false}
					dot={false}
				/>
				<Line
					yAxisId="right"
					type="monotone"
					dataKey="current"
					name="Milliamps"
					stroke="cyan"
					isAnimationActive={false}
					dot={false}
				/>
			</LineChart>
		</ResponsiveContainer>
	);
}
