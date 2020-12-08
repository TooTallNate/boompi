import ms from 'ms';
import createDebug from 'debug';
import { useCallback, useRef } from 'react';

import { Battery } from '@lib/types';

import {
	CartesianGrid,
	LineChart,
	ReferenceLine,
	ResponsiveContainer,
	Label,
	XAxis,
	YAxis,
	AxisDomain,
	Tooltip,
	Legend,
	Line,
} from 'recharts';

interface BatteryChartProps {
	battery: Battery;
}

const debug = createDebug('boompi:components:battery-chart');

export default function BatteryChart({ battery }: BatteryChartProps) {
	const now = Date.now();

	const history = useRef<Map<number, Battery>>(new Map());
	if (!history.current.has(battery.date)) {
		history.current.set(battery.date, battery);
	}

	for (const batt of history.current.values()) {
		if (now - batt.date > ms('1m')) {
			history.current.delete(batt.date);
		}
	}

	const xDomain: [AxisDomain, AxisDomain] = [
		useCallback(() => {
			return now - ms('1m');
		}, [now]),
		useCallback(() => {
			return now + ms('5s');
		}, [now]),
	];

	const data = Array.from(history.current.values());

	return (
		<ResponsiveContainer height="100%" width="100%">
			<LineChart data={data} margin={{ left: -10, right: -10 }}>
				<XAxis
					height={40}
					dataKey="date"
					tickFormatter={(val) => {
						const diff = now - val;
						if (diff < 1000) return '';
						return `${ms(now - val)} ago`;
					}}
					allowDataOverflow={true}
					tick={{ fontSize: 10 }}
					domain={xDomain}
				>
					<Label
						value="Timestamp"
						position="insideBottom"
						fontSize={14}
						fill="#676767"
					/>
				</XAxis>
				<YAxis
					width={80}
					yAxisId="left"
					tick={{ fontSize: 10 }}
					domain={[17, 26]}
				>
					<Label
						value="Volts"
						angle={-90}
						position="outside"
						fill="#676767"
						fontSize={14}
					/>
				</YAxis>
				<YAxis
					width={80}
					yAxisId="right"
					orientation="right"
					tick={{ fontSize: 10 }}
					domain={[50, 300]}
				>
					<Label
						value="Milliamps"
						angle={90}
						position="outside"
						fill="#676767"
						fontSize={14}
					/>
				</YAxis>
				<Tooltip />
				<Line
					yAxisId="left"
					type="monotone"
					dataKey={'voltage'}
					stroke={'green'}
					isAnimationActive={false}
					dot={false}
				/>
				<Line
					yAxisId="right"
					type="monotone"
					dataKey={'current'}
					stroke={'cyan'}
					isAnimationActive={false}
					dot={false}
				/>
			</LineChart>
		</ResponsiveContainer>
	);
}