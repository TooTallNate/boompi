import styles from '@styles/now-playing.module.css';
import * as Progress from '@radix-ui/react-progress';

function formatSeconds(ms: number) {
	const minutes = Math.floor(ms / (1000 * 60));
	const seconds = Math.floor((ms - minutes * (1000 * 60)) / 1000);
	return `${minutes}:${String(seconds).padStart(2, '0')}`;
}

interface TrackPositionProps {
	duration: number;
	playPosition: number;
}

export default function NowPlaying({
	duration,
	playPosition,
}: TrackPositionProps) {
	const indicator = (
		<Progress.Indicator
			as="div"
			// @ts-ignore
			style={{ width: `${(playPosition / duration) * 100}%` }}
		/>
	);
	return (
		<div className={styles.position}>
			<div className={styles.label}>{formatSeconds(playPosition)}</div>
			{
				// @ts-ignore
				<Progress.Root
					as="div"
					className={styles.progress}
					value={playPosition / duration}
					max={1}
				>
					{indicator}
				</Progress.Root>
			}
			<div className={styles.label}>
				-{formatSeconds(duration - playPosition)}
			</div>
		</div>
	);
}
