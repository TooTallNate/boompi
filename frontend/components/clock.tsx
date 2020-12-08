import format from 'dateformat';
import styles from '@styles/clock.module.css';

interface ClockProps {
	date: Date;
}

export default function Clock({ date }: ClockProps) {
	const hours = format(date, 'H');
	const minutes = format(date, 'MM');
	const opacity = date.getSeconds() % 2 ? 1 : 0.2;
	return (
		<span className={styles.clock}>
			<span className={styles.hours}>{hours}</span>
			<span className={styles.divider} style={{ opacity }}>
				:
			</span>
			<span className={styles.minutes}>{minutes}</span>
		</span>
	);
}
