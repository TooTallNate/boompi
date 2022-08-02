import createDebug from 'debug';
import ReconnectingWebSocket from 'reconnecting-websocket';
import { useCallback, useEffect, useState, useRef } from 'react';

import { Battery } from '@lib/types';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const wsRef = useRef<ReconnectingWebSocket>();
	const [prettyHostname, setPrettyHostname] = useState<string>('Unknown');
	const [battery, setBattery] = useState<Battery | null>(null);
	const [volume, setVolume] = useState<number | null>(null);
	const [bluetoothName, setBluetoothName] = useState<string | null>(null);
	const [artist, setArtist] = useState('');
	const [track, setTrack] = useState('');
	const [album, setAlbum] = useState('');
	const [position, setPosition] = useState(0);
	const [positionChangedAt, setPositionChangedAt] = useState(0);
	const [duration, setDuration] = useState(0);
	const [isPlaying, setIsPlaying] = useState(false);
	const [webSocketConnected, setWebSocketConnected] = useState(false);

	const onMessage = useCallback((event: MessageEvent) => {
		console.log(event.data);
		if (typeof event.data === 'string') {
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
				setPositionChangedAt(body.updatedAt || Date.now());
			}
			if (typeof body.duration === 'number') {
				setDuration(body.duration);
			}
			if (typeof body.prettyHostname === 'string') {
				setPrettyHostname(body.prettyHostname);
			}
			if (typeof body.status === 'string') {
				if (body.status === 'playing') {
					setIsPlaying(true);
				} else {
					// paused / stopped
					setIsPlaying(false);
				}
			}
			if (body.battery) {
				setBattery({ ...body.battery, date: Date.now() });
			}
		}
	}, []);

	const onOpen = useCallback(() => {
		debug('WebSocket "open" event');
		setWebSocketConnected(true);
	}, []);

	const onClose = useCallback(() => {
		debug('WebSocket "close" event');
		setWebSocketConnected(false);
	}, []);

	const onError = useCallback(() => {
		debug('WebSocket "error" event');
		setWebSocketConnected(false);
	}, []);

	useEffect(() => {
		debug('Creating reconnecting WebSocket connection: %o', url);
		const socket = new ReconnectingWebSocket(url);
		socket.addEventListener('message', onMessage);
		socket.addEventListener('open', onOpen);
		socket.addEventListener('close', onClose);
		socket.addEventListener('error', onError);
		wsRef.current = socket;
		return () => {
			debug('Closing WebSocket connection');
			wsRef.current = undefined;
			socket.close();
		};
	}, [url, onMessage, onOpen, onClose, onError]);

	return {
		webSocketConnected,
		prettyHostname,
		battery,
		volume,
		bluetoothName,
		artist,
		album,
		track,
		position,
		positionChangedAt,
		duration,
		isPlaying,
		setVolume: useCallback((value: number) => {
			debug('Setting volume: %o', value);
			wsRef.current?.send(JSON.stringify({ volume: value }));
			setVolume(value);
		}, []),
		setPlay: useCallback(() => {
			debug('Playing');
			wsRef.current?.send(JSON.stringify({ play: true }));
			setPositionChangedAt(Date.now());
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
		batteryFastPoll: useCallback((enabled: boolean) => {
			debug('Battery fast poll: %o', enabled);
			wsRef.current?.send(JSON.stringify({ batteryFastPoll: enabled }));
		}, []),
	};
}
