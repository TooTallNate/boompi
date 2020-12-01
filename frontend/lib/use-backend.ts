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
		null
	);
	const [artist, setArtist] = useState('');
	const [track, setTrack] = useState('');
	const [album, setAlbum] = useState('');
	const [position, setPosition] = useState(0);
	const [duration, setDuration] = useState(0);
	const [isCharging, setIsCharging] = useState(false);
	const [isPlaying, setIsPlaying] = useState(false);

	const onMessage = useCallback((event: MessageEvent) => {
		const body = JSON.parse(event.data);
		if ('bluetoothName' in body) {
			setBluetoothName(body.bluetoothName);
		}
		if (typeof body.volume === 'number') {
			setVolume(body.volume);
		}
		if (typeof body.artist === 'string') {
			setArtist(body.artist);
		}
		if (typeof body.track === 'string') {
			setTrack(body.track);
		}
		if (typeof body.album === 'string') {
			setAlbum(body.album);
		}
		if (typeof body.position === 'number') {
			setPosition(body.position);
		}
		if (typeof body.duration === 'number') {
			setDuration(body.duration);
		}
		if (typeof body.status === 'string') {
			if (body.status === 'playing') {
				setIsPlaying(true);
			} else {
				// paused / stopped
				setIsPlaying(false);
			}
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
		isPlaying,
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
			setIsPlaying(true);
		}, []),
		setPause: useCallback(() => {
			debug('Pausing');
			wsRef.current?.send(JSON.stringify({ pause: true }));
			setIsPlaying(false);
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
