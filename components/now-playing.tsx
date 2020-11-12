// CSS
import styles from '@styles/now-playing.module.css';

// Icons
import Play from '@components/icons/play';
import Previous from '@components/icons/previous';
import Next from '@components/icons/next';

export default function NowPlaying({}) {
	console.log(styles);
	return <div className={styles.nowPlaying}>
		<div className={styles.artist}>Pink Floyd</div>
		<div className={styles.track}>Comfortably Numb</div>
		<div className={styles.album}>The Wall</div>
		<div className={styles.position}>
			<hr />
			00'00"
			<hr />
		</div>
		<div className={styles.controls}>
			<Previous />
			<Play />
			<Next />
		</div>
	</div>
}
