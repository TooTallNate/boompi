import createDebug from 'debug';
import ReconnectingWebSocket from 'reconnecting-websocket';
import { useCallback, useEffect, useState, useRef } from 'react';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const wsRef = useRef<ReconnectingWebSocket>();
	const [battery, setBattery] = useState(0.95);
	const [volume, setVolume] = useState(0);
	const [artist, setArtist] = useState('Pink Floyd');
	const [track, setTrack] = useState('Comfortably Numb');
	const [album, setAlbum] = useState('The Wall');
	const [position, setPosition] = useState(0);
	const [duration, setDuration] = useState(143 * 1000);
	const [isCharging, setIsCharging] = useState(false);
	const [isPlaying, setIsPlaying] = useState(false);

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
		setPosition: useCallback((value: number) => {
			debug('Setting track position: %o', value);
			wsRef.current?.send(JSON.stringify({ position: value }));
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
