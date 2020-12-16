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

	useEffect(() => {
		if (clicksData.length === 0) return;
		// Sleep for the duration of the CSS effect
		const t = setTimeout(() => {
			setClicksData([]);
		}, 600);
		return () => {
			clearTimeout(t);
		};
	}, [clicksData]);

	const clicks = clicksData.map((data) => (
		<div
			key={data.date}
			className={styles.click}
			style={{ top: `${data.y}px`, left: `${data.x}px` }}
		/>
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
