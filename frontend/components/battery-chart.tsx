import ms from 'ms';
import { useState, useEffect } from 'react';

import { Battery } from '@lib/types';
import {
	formatAmps,
	calculateAmpsMinDomain,
	calculateAmpsMaxDomain,
	millisToMinutesAndSeconds,
} from '@lib/utils';

import {
	LineChart,
	ResponsiveContainer,
	Label,
	XAxis,
	YAxis,
	AxisDomain,
	Line,
} from 'recharts';

interface BatteryChartProps {
	battery: Battery;
}

const history = new Map<number, Battery>();

export default function BatteryChart({ battery }: BatteryChartProps) {
	const [now, setNow] = useState(Date.now());
	const [lookback] = useState(ms('3m'));

	// Update the position of the chart 5 times per second
	useEffect(() => {
		const t = setTimeout(() => {
			setNow(Date.now());
		}, 200);
		return () => clearTimeout(t);
	}, [now]);

	if (!history.has(battery.date)) {
		// Purge old battery readings
		const ageThreshold = lookback + 2000;
		for (const batt of history.values()) {
			if (now - batt.date > ageThreshold) {
				history.delete(batt.date);
			}
		}

		// Add the new battery reading
		history.set(battery.date, battery);
	}

	const xStart = now - lookback;
	const xDomain: [AxisDomain, AxisDomain] = [() => xStart, () => now];
	const amperageDomain: [AxisDomain, AxisDomain] = [
		calculateAmpsMinDomain,
		calculateAmpsMaxDomain,
	];

	const data = Array.from(history.values());

	return (
		<ResponsiveContainer height="100%" width="97.6%">
			<LineChart data={data}>
				<XAxis
					height={40}
					dataKey="date"
					type="number"
					tickFormatter={(val) => {
						return millisToMinutesAndSeconds(now - val);
					}}
					allowDecimals={false}
					allowDataOverflow={true}
					tick={{ fontSize: 15 }}
					stroke="#aaa"
					domain={xDomain}
					tickCount={7}
				></XAxis>
				<YAxis
					width={80}
					yAxisId="left"
					tick={{ fontSize: 14 }}
					stroke="#aaa"
					allowDecimals={false}
					domain={[18, 25]}
					tickCount={8}
					padding={{ top: 20, bottom: 20 }}
					unit="v"
				>
					<Label
						value="Voltage"
						angle={-90}
						position="outside"
						dx={-15}
						fill="white"
						fontSize={18}
					/>
				</YAxis>
				<YAxis
					width={80}
					yAxisId="right"
					orientation="right"
					tick={{ fontSize: 13 }}
					stroke="#aaa"
					allowDecimals={false}
					padding={{ top: 20, bottom: 20 }}
					domain={amperageDomain}
					tickFormatter={formatAmps}
					tickCount={8}
				>
					<Label
						value="Amperage"
						angle={90}
						position="outside"
						dx={40}
						fill="white"
						fontSize={18}
					/>
				</YAxis>
				<Line
					yAxisId="left"
					type="monotone"
					dataKey="voltage"
					name="Volts"
					stroke="magenta"
					isAnimationActive={false}
					dot={false}
				/>
				<Line
					yAxisId="right"
					type="monotone"
					dataKey="current"
					name="Current"
					stroke="cyan"
					isAnimationActive={false}
					dot={false}
				/>
			</LineChart>
		</ResponsiveContainer>
	);
}
