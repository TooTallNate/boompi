import createDebug from 'debug';
import { useCallback, useEffect, useState } from 'react';
import ReconnectingWebSocket from 'reconnecting-websocket';

const debug = createDebug('boompi:lib:use-backend');

interface UseBackendOptions {
	url: string;
}

export default function useBackend({ url }: UseBackendOptions) {
	const [ ws, setWs ] = useState(null);
	const [ volume, setVolumeState ] = useState(0);

	const setVolume = useCallback((value: number) => {
		debug('Setting volume: %o', value);
		ws?.send(JSON.stringify({ volume: value }));
		setVolumeState(value);
	}, [ws]);

	useEffect(() => {
		debug('Creating WebSocket connection: %o', url);
		const socket = new ReconnectingWebSocket(url);
		setWs(socket);
		return () => {
			debug('Closing WebSocket connection');
			socket.close();
			setWs(null);
		};
	}, [url]);

	return {
		volume,
		setVolume
	};
}
