import ms from 'ms';
import createDebug from 'debug';
import { useCallback, useRef } from 'react';

import { Battery } from '@lib/types';

import {
	CartesianGrid,
	LineChart,
	ReferenceLine,
	ResponsiveContainer,
	XAxis,
	YAxis,
	AxisDomain,
	Tooltip,
	Legend,
	Line,
} from 'recharts';

// Hooks
import useNow from '@lib/use-now';

interface BatteryChartProps {
	battery: Battery;
}

const debug = createDebug('boompi:components:battery-chart');

export default function BatteryChart({ battery }: BatteryChartProps) {
	const { now } = useNow();

	const history = useRef<Map<number, Battery>>(new Map());
	if (!history.current.has(battery.date)) {
		history.current.set(battery.date, battery);
	}

	const xDomain: [AxisDomain, AxisDomain] = [
		useCallback(() => {
			return now - ms('1m');
		}, [now]),
		useCallback(() => {
			return now - ms('5s');
		}, [now]),
	];

	const data = Array.from(history.current.values());

	return (
		<ResponsiveContainer height="100%" width="100%">
			<LineChart
				data={data}
				margin={{ top: 5, right: 15, left: 0, bottom: 5 }}
			>
				<CartesianGrid strokeDasharray="3 3" />
				<XAxis
					dataKey="date"
					type="number"
					//tickFormatter={formatHoursMinutes}
					allowDataOverflow={true}
					domain={xDomain}
				/>
				<YAxis type="number" domain={[17, 26]} />
				<Legend />

				<Line
					type="monotone"
					dataKey="voltage"
					stroke="green"
					isAnimationActive={false}
				/>

				<Line
					type="monotone"
					dataKey="current"
					stroke="blue"
					isAnimationActive={false}
				/>

				<ReferenceLine x={now} stroke="#333" />
			</LineChart>
		</ResponsiveContainer>
	);
}
