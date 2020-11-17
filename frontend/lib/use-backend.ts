import createDebug from 'debug';
import { useCallback, useEffect, useState } from 'react';
import ReconnectingWebSocket from 'reconnecting-websocket';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const [ws, setWs] = useState<ReconnectingWebSocket | null>(null);
	const [volume, setVolume] = useState(0);
	const [artist, setArtist] = useState('Pink Floyd');
	const [track, setTrack] = useState('Comfortably Numb');
	const [album, setAlbum] = useState('The Wall');
	const [position, setPosition] = useState(0);
	const [duration, setDuration] = useState(143 * 1000);
	const [isPlaying, setIsPlaying] = useState(true);

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
		setWs(socket);
		return () => {
			debug('Closing WebSocket connection');
			socket.close();
			setWs(null);
		};
	}, [url, onMessage]);

	return {
		volume,
		artist,
		album,
		track,
		position,
		duration,
		isPlaying,
		setVolume: useCallback(
			(value: number) => {
				debug('Setting volume: %o', value);
				ws?.send(JSON.stringify({ volume: value }));
				setVolume(value);
			},
			[ws]
		),
		setPosition: useCallback(
			(value: number) => {
				debug('Setting track position: %o', value);
				ws?.send(JSON.stringify({ position: value }));
				setPosition(value);
			},
			[ws]
		),
		setPlay: useCallback(() => {
			debug('play');
			setIsPlaying(true);
		}, [ws]),
		setPause: useCallback(() => {
			debug('pause');
			setIsPlaying(false);
		}, [ws]),
		setRewind: useCallback(() => {
			debug('rewind');
		}, [ws]),
		setFastForward: useCallback(() => {
			debug('fast forward');
		}, [ws]),
	};
}
