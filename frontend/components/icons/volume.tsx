interface VolumeParams extends React.ComponentPropsWithoutRef<'svg'> {
	mute?: boolean;
	level?: number;
}

export default function Volume(_props: VolumeParams) {
	const { mute = false, level = 0, ...props } = _props;
	let muted;
	let level1;
	let level2;
	let level3;
	if (mute) {
		muted = (
			<path
				className="muted"
				d="M90.4 59.6L80.8 50l9.6-9.6c2.1-2.1-1.2-5.4-3.3-3.3l-9.6 9.6-9.6-9.6c-2.1-2.1-5.4 1.2-3.3 3.3l9.6 9.6-9.6 9.6c-2.1 2.1 1.2 5.4 3.3 3.3l9.6-9.6 9.6 9.6c2.1 2.1 5.4-1.2 3.3-3.3z"
			/>
		);
	} else {
		if (level >= 1) {
			level1 = (
				<path
					className="level-1"
					d="M60.2 39.1c5.9 6.4 5.9 15.4 0 21.7-2.4 2.6 1.5 6.4 3.8 3.8 7.9-8.6 7.9-20.8 0-29.4-2.3-2.5-6.2 1.4-3.8 3.9z"
				/>
			);
		}
		if (level >= 2) {
			level2 = (
				<path
					className="level-2"
					d="M68.6 32c9.7 10.4 9.7 25.5 0 35.9-2.4 2.6 1.5 6.4 3.8 3.8 11.7-12.6 11.7-31 0-43.6-2.3-2.5-6.1 1.4-3.8 3.9z"
				/>
			);
		}
		if (level >= 3) {
			level3 = (
				<path
					className="level-3"
					d="M77.1 25c13.5 14.5 13.5 35.6 0 50.1-2.4 2.5 1.4 6.4 3.8 3.8 15.6-16.7 15.6-41.1 0-57.8-2.4-2.5-6.2 1.3-3.8 3.9z"
				/>
			);
		}
	}
	return (
		<svg
			xmlns="http://www.w3.org/2000/svg"
			viewBox="0 0 100 100"
			{...props}
		>
			<path d="M52.8 24v52c0 4.7-5.6 7.2-9.1 4L26.3 64.2H12c-3 0-5.4-2.4-5.4-5.4V41.2c0-3 2.4-5.4 5.4-5.4h14.3L43.7 20c3.5-3.2 9.1-.7 9.1 4z" />
			{level1}
			{level2}
			{level3}
			{muted}
		</svg>
	);
}
