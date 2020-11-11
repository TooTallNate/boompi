/**
 * Battery by Alice Design from the Noun Project.
 * https://thenounproject.com/rose-alice-design/collection/battery/
 */

interface BatteryParams extends React.ComponentPropsWithoutRef<'svg'> {
	charging?: boolean;
	percentage?: number;
}

export default function Battery(_props: BatteryParams) {
	const { charging = false, percentage = 1, ...props } = _props;
	let charge;
	let fill;
	if (charging) {
		charge = (
			<path
				className="charge"
				d="M64.4 45.9l-20.1-4.8c-1.1-.3-2.2 0-3.1.7-.9.7-1.4 1.7-1.4 2.9v6.1L24.3 47c-2-.5-3.9.7-4.4 2.7-.5 2 .7 3.9 2.7 4.4l20.1 4.8c.3.1.6.1.9.1.8 0 1.6-.3 2.2-.8.9-.7 1.4-1.7 1.4-2.9v-6.1L62.7 53c2 .5 3.9-.7 4.4-2.7.5-1.9-.7-3.9-2.7-4.4z"
			/>
		);
	}
	if (percentage >= 0.95) {
		// Full
		fill = (
			<path
				className="fill"
				d="M74.9 35.6H17.7c-1.4 0-2.6 1.2-2.6 2.6v23.7c0 1.4 1.2 2.6 2.6 2.6h57.2c1.4 0 2.6-1.2 2.6-2.6V38.1c0-1.4-1.1-2.5-2.6-2.5z"
			/>
		);
	} else if (percentage >= 0.75) {
		fill = (
			<path
				className="fill"
				d="M58.8 37.1c-.4-.9-1.3-1.5-2.4-1.5H17.7c-1.4 0-2.6 1.2-2.6 2.6v23.7c0 1.4 1.2 2.6 2.6 2.6H67c1.9 0 3.1-1.9 2.4-3.6L58.8 37.1z"
			/>
		);
	} else if (percentage >= 0.5) {
		fill = (
			<path
				className="fill"
				d="M45.8 37.1c-.4-.9-1.3-1.5-2.4-1.5H17.7c-1.4 0-2.6 1.2-2.6 2.6v23.7c0 1.4 1.2 2.6 2.6 2.6h36.4c1.9 0 3.1-1.9 2.4-3.6L45.8 37.1z"
			/>
		);
	} else if (percentage >= 0.2) {
		fill = (
			<path
				className="fill"
				d="M26.4 37.1c-.4-.9-1.3-1.5-2.4-1.5h-6.3c-1.4 0-2.6 1.2-2.6 2.6v23.7c0 1.4 1.2 2.6 2.6 2.6h17c1.9 0 3.1-1.9 2.4-3.6L26.4 37.1z"
			/>
		);
	}
	return (
		<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
			<path d="M93 37.1h-2.8v-.9c0-7.3-6-13.3-13.3-13.3H15.8c-7.3 0-13.3 6-13.3 13.3v27.6c0 7.3 6 13.3 13.3 13.3h61.1c7.3 0 13.3-6 13.3-13.3v-.9H93c2.5 0 4.5-2 4.5-4.5V41.7c0-2.5-2-4.6-4.5-4.6zm-8.7 26.7c0 4.1-3.3 7.4-7.4 7.4H15.8c-4.1 0-7.4-3.3-7.4-7.4V36.2c0-4.1 3.3-7.4 7.4-7.4h61.1c4.1 0 7.4 3.3 7.4 7.4v27.6z" />
			{fill}
			{charge}
		</svg>
	);
}
