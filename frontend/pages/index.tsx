import Head from 'next/head';

// CSS
import styles from '@styles/index.module.css';

// Components
import Header from '@components/header';
import NowPlaying from '@components/now-playing';
import ConnectBluetooth from '@components/connect-bluetooth';
import WebSocketConnecting from '@components/websocket-connecting';

// Hooks
import useNow from '@lib/use-now';
import useBackend from '@lib/use-backend';

export default function Index() {
	const { now } = useNow();
	const {
		webSocketConnected,
		battery,
		volume,
		bluetoothName,
		artist,
		track,
		album,
		position,
		duration,
		isCharging,
		isPlaying,
		setVolume,
		setPlay,
		setPause,
		setRewind,
		setFastForward,
	} = useBackend({
		url: 'ws://boompi.local:3001',
	});

	let content = null;

	if (webSocketConnected) {
		if (typeof bluetoothName === 'string') {
			content = (
				<NowPlaying
					artist={artist}
					track={track}
					album={album}
					volume={volume}
					position={position}
					duration={duration}
					isPlaying={isPlaying}
					onVolumeChange={setVolume}
					onPlay={setPlay}
					onPause={setPause}
					onRewind={setRewind}
					onFastForward={setFastForward}
				/>
			);
		} else {
			content = <ConnectBluetooth name="Nathan's 🔊" />;
		}
	} else {
		content = <WebSocketConnecting />;
	}

	return (
		<>
			<Head>
				<title>Boompi</title>
				<meta
					name="viewport"
					content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no"
				/>
			</Head>
			<main className={styles.main}>
				<Header
					now={now}
					bluetoothName={bluetoothName}
					isCharging={isCharging}
					battery={battery}
					volume={volume}
				/>
				<section className={styles.content}>{content}</section>
			</main>
		</>
	);
}
