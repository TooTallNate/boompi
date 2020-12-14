import { useCallback, useState } from 'react';

// CSS
import styles from '@styles/click-effect.module.css';

interface ClickEffectProps {
	children: any;
}

interface Click {
	x: number;
	y: number;
	date: number;
}

export default function ClickEffect({ children }: ClickEffectProps) {
	const [clicksData, setClicksData] = useState<Click[]>([]);
	function handleClick(e) {
		const click = {
			x: e.clientX,
			y: e.clientY,
			date: Date.now(),
		};
		setClicksData([...clicksData, click]);
	}
	const clicks = clicksData.map((data) => (
		<div
			data-date={data.date}
			className={styles.click}
			style={{ top: `${data.y}px`, left: `${data.x}px` }}
		/>
	));
	return (
		<div onClick={handleClick}>
			{children}
			{clicks}
		</div>
	);
}
