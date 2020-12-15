import { useCallback, useEffect, useState } from 'react';

// CSS
import styles from '@styles/click-effect.module.css';

interface ClickEffectProps {
	children: any;
}

interface ClickData {
	x: number;
	y: number;
	date: number;
}

interface ClickProps extends ClickData {
	onEffectFinished: (date: number) => void;
}

function Click({ x, y, date, onEffectFinished }: ClickProps) {
	useEffect(() => {
		const t = setTimeout(() => {
			onEffectFinished(date);
		}, 600);
		return () => {
			clearTimeout(t);
		};
	}, [date]);
	return (
		<div
			key={date}
			className={styles.click}
			style={{ top: `${y}px`, left: `${x}px` }}
		/>
	);
}

export default function ClickEffect({ children }: ClickEffectProps) {
	const [clicksData, setClicksData] = useState<ClickData[]>([]);

	const handleClick = useCallback(
		(e) => {
			const clicks: ClickData[] = [];
			if (Array.isArray(e.touches)) {
				for (const t of e.touches) {
					clicks.push({
						x: t.clientX,
						y: t.clientY,
						date: Date.now(),
					});
				}
			} else if (typeof e.clientX === 'number') {
				clicks.push({
					x: e.clientX,
					y: e.clientY,
					date: Date.now(),
				});
			}
			setClicksData([...clicksData, ...clicks]);
		},
		[clicksData]
	);

	const handleEffectFinished = useCallback(
		(date: number) => {
			setClicksData(clicksData.filter((d) => d.date !== date));
		},
		[clicksData]
	);

	const clicks = clicksData.map((data) => (
		<Click onEffectFinished={handleEffectFinished} {...data} />
	));

	return (
		<div
			className={styles.wrapper}
			onMouseDown={handleClick}
			onTouchStart={handleClick}
		>
			{children}
			{clicks}
		</div>
	);
}
