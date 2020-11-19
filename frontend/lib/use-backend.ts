import createDebug from 'debug';
import ReconnectingWebSocket from 'reconnecting-websocket';
import { useCallback, useEffect, useState, useRef } from 'react';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const wsRef = useRef<ReconnectingWebSocket>();
	const [battery, setBattery] = useState(0.8);
	const [volume, setVolume] = useState(0);
	const [bluetoothName, setBluetoothName] = useState<string | null>(
		"Nathan's iPhone"
		//null
	);
	const [artist, setArtist] = useState('Pink Floyd');
	const [track, setTrack] = useState('Comfortably Numb');
	const [album, setAlbum] = useState('The Wall');
	const [position, setPosition] = useState(0);
	const [duration, setDuration] = useState(360 * 1000);
	const [isCharging, setIsCharging] = useState(true);
	const [playingStart, setPlayingStart] = useState<Date | null>(null);

	const onMessage = useCallback((event: MessageEvent) => {
		const body = JSON.parse(event.data);
		if (typeof body.volume === 'number') {
			setVolume(body.volume);
		}
	}, []);

	useEffect(() => {
		debug('Creating WebSocket connection: %o', url);
		const socket = new ReconnectingWebSocket(url);
		socket.addEventListener('message', onMessage);
		wsRef.current = socket;
		return () => {
			debug('Closing WebSocket connection');
			wsRef.current = undefined;
			socket.close();
		};
	}, [url, onMessage]);

	return {
		battery,
		volume,
		bluetoothName,
		artist,
		album,
		track,
		position,
		duration,
		isCharging,
		playingStart,
		setVolume: useCallback((value: number) => {
			debug('Setting volume: %o', value);
			wsRef.current?.send(JSON.stringify({ volume: value }));
			setVolume(value);
		}, []),
		setPosition: useCallback((value: number, clientSide = false) => {
			debug('Setting track position: %o', value);
			if (!clientSide) {
				wsRef.current?.send(JSON.stringify({ position: value }));
			}
			setPosition(value);
		}, []),
		setPlay: useCallback(() => {
			debug('Playing');
			wsRef.current?.send(JSON.stringify({ play: true }));
			setPlayingStart(new Date());
		}, []),
		setPause: useCallback(() => {
			debug('Pausing');
			wsRef.current?.send(JSON.stringify({ pause: true }));
			setPlayingStart(null);
		}, []),
		setRewind: useCallback(() => {
			debug('Rewinding');
			wsRef.current?.send(JSON.stringify({ rewind: true }));
		}, []),
		setFastForward: useCallback(() => {
			debug('Fast forwarding');
			wsRef.current?.send(JSON.stringify({ fastForward: true }));
		}, []),
	};
}
