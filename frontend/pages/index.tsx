import Head from 'next/head';
import { useCallback, useState } from 'react';

// CSS
import styles from '@styles/index.module.css';

// Components
import Battery from '@components/battery';
import Header from '@components/header';
import NowPlaying from '@components/now-playing';
import ConnectBluetooth from '@components/connect-bluetooth';
import WebSocketConnecting from '@components/websocket-connecting';

// Hooks
import useBackend from '@lib/use-backend';

export default function Index() {
	const [panel, setPanel] = useState('');
	const {
		webSocketConnected,
		battery,
		volume,
		bluetoothName,
		artist,
		track,
		album,
		position,
		positionChangedAt,
		duration,
		isCharging,
		isPlaying,
		setVolume,
		setPlay,
		setPause,
		setRewind,
		setFastForward,
		batteryFastPoll,
	} = useBackend({
		url: 'ws://boompi.local:3001',
	});

	const showBatteryPanel = useCallback(() => {
		setPanel('battery');
	}, []);

	const closePanel = useCallback(() => {
		setPanel('');
	}, []);

	let content = null;

	if (webSocketConnected) {
		if (panel === 'battery' && battery) {
			content = (
				<Battery battery={battery} batteryFastPoll={batteryFastPoll} />
			);
		} else if (typeof bluetoothName === 'string') {
			content = (
				<NowPlaying
					artist={artist}
					track={track}
					album={album}
					volume={volume}
					position={position}
					positionChangedAt={positionChangedAt}
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
					bluetoothName={bluetoothName}
					isCharging={isCharging}
					battery={battery}
					volume={volume}
					onBatteryClick={
						panel === 'battery' ? closePanel : showBatteryPanel
					}
				/>
				<section className={styles.content}>{content}</section>
			</main>
		</>
	);
}
