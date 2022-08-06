import styles from '@styles/visualizer.module.css';

export interface VisualizerProps {
	data?: ArrayLike<number>;
	max: number;
}

export function Visualizer({ data, max }: VisualizerProps) {
	return (
		<div className={styles.visualizer}>
			{data &&
				Array.from(data).map((val, i) => {
					const percent = (val ?? 0) / max;
					return (
						<div
							key={i}
							className={styles.bar}
							style={{
								height: `${percent * 100}%`,
							}}
						></div>
					);
				})}
		</div>
	);
}