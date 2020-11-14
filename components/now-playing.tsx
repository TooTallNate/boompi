// CSS
import styles from '@styles/now-playing.module.css';

// Icons
import Play from '@components/icons/play';
import Rewind from '@components/icons/rewind';
import Volume from '@components/icons/volume';
import FastForward from '@components/icons/fast-forward';

export default function NowPlaying({}) {
	function onPlay(...args) {
		console.log('onPlay', args);
	}
	return <div className={styles.nowPlaying}>
		<div className={styles.artist}>Pink Floyd</div>
		<div className={styles.track}>Comfortably Numb</div>
		<div className={styles.album}>The Wall</div>
		<div className={styles.position}>
			1:36
			<input type="range" min="0" max="100" />
			-2:14
		</div>
		<div className={styles.controls}>
			<Rewind />
			<Play onClick={onPlay} />
			<FastForward />
		</div>
		<div className={styles.volume}>
			<Volume />
			<input type="range" min="0" max="100" />
			<Volume level={3} />
		</div>
	</div>
}
