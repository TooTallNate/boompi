import createDebug from 'debug';
import { useCallback, useEffect, useState } from 'react';
import ReconnectingWebSocket from 'reconnecting-websocket';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const [ws, setWs] = useState(null);
	const [volume, setVolumeState] = useState(0);

	const setVolume = useCallback(
		(value: number, backend = true) => {
			debug('Setting volume: %o', value);
			if (backend) {
				ws?.send(JSON.stringify({ volume: value }));
			}
			setVolumeState(value);
		},
		[ws]
	);

	const onMessage = useCallback((event: MessageEvent) => {
		const body = JSON.parse(event.data);
		console.log({ body });
		if (typeof body.volume === 'number') {
			setVolume(body.volume, false);
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
		setVolume,
	};
}
