import Head from 'next/head';
import * as fs from 'fs/promises';
import * as dotenv from 'dotenv';
import { useCallback, useState } from 'react';

// CSS
import styles from '@styles/index.module.css';

// Components
import Battery from '@components/battery';
import Footer from '@components/footer';
import NowPlaying from '@components/now-playing';
import ClickEffect from '@components/click-effect';
import ConnectBluetooth from '@components/connect-bluetooth';
import WebSocketConnecting from '@components/websocket-connecting';

// Hooks
import useBackend from '@lib/use-backend';

const BACKEND_HOSTNAME =
	process.env.NEXT_PUBLIC_BACKEND_HOSTNAME || '127.0.0.1';
const BACKEND_PORT = process.env.NEXT_PUBLIC_BACKEND_PORT || '3001';

export async function getStaticProps() {
	const machineInfo = dotenv.parse(
		await fs.readFile('/etc/machine-info', 'utf8')
	);
	return {
		props: {
			bluetoothHostname: machineInfo.PRETTY_HOSTNAME || 'Unknown',
		},
	};
}

interface IndexProps {
	bluetoothHostname: string;
}

export default function Index({ bluetoothHostname }: IndexProps) {
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
		isPlaying,
		setVolume,
		setPlay,
		setPause,
		setRewind,
		setFastForward,
		batteryFastPoll,
	} = useBackend({
		url: `ws://${BACKEND_HOSTNAME}:${BACKEND_PORT}`,
	});

	const showBatteryPanel = useCallback(() => {
		setPanel('battery');
	}, []);

	const closePanel = useCallback(() => {
		setPanel('');
	}, []);

	const handleRefresh = useCallback(() => {
		window.location.reload();
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
			content = <ConnectBluetooth name={bluetoothHostname} />;
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
				<ClickEffect>
					<section className={styles.content}>{content}</section>
					<Footer
						bluetoothName={bluetoothName}
						battery={battery}
						volume={volume}
						onBatteryClick={
							panel === 'battery' ? closePanel : showBatteryPanel
						}
					/>
					<div className={styles.refresh} onClick={handleRefresh} />
				</ClickEffect>
			</main>
		</>
	);
}
