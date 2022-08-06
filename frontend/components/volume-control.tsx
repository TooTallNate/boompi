import { useCallback } from 'react';
import * as Slider from '@radix-ui/react-slider';

import Volume from '@components/icons/volume';

import styles from '@styles/now-playing.module.css';

interface VolumeControlProps {
	volume: number;
	onVolumeChange: (value: number) => void;
}

export default function VolumeControl({
	volume,
	onVolumeChange,
}: VolumeControlProps) {
	const onVolume = useCallback(
		([vol]: number[]) => {
			onVolumeChange(vol / 100);
		},
		[onVolumeChange]
	);

	// @ts-ignore
	const range = <Slider.Range as="span" className={styles.volumeRange} />;

	const track = (
		// @ts-ignore
		<Slider.Track as="span" className={styles.volumeTrack}>
			{range}
		</Slider.Track>
	);

	// @ts-ignore
	const thumb = <Slider.Thumb as="span" className={styles.volumeThumb} />;

	const slider = (
		// @ts-ignore
		<Slider.Root
			as="span"
			className={styles.volumeSlider}
			min={0}
			max={100}
			value={[volume * 100]}
			onValueChange={onVolume}
		>
			{track}
			{thumb}
		</Slider.Root>
	);

	return (
		<div className={styles.volume}>
			<div className={styles.label}>
				<Volume level={1} className={styles.volumeMin} />
			</div>
			<div className={styles.slider}>{slider}</div>
			<div className={styles.label}>
				<Volume level={3} />
			</div>
		</div>
	);
}
