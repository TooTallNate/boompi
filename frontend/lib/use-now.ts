import createDebug from 'debug';
import { useEffect, useState } from 'react';

const debug = createDebug('boompi:lib:use-now');

export default function useNow() {
	const [now, setNow] = useState(new Date());
	useEffect(() => {
		debug('Creating "Now" hook');
		function resetTimeout(now: number) {
			const ms = 1000 - (now % 1000);
			return setTimeout(tick, ms);
		}
		function tick() {
			const now = new Date();
			timeout = resetTimeout(+now);
			setNow(now);
		}
		let timeout = resetTimeout(Date.now());
		return () => {
			debug('Cleaning up "Now" hook');
			clearTimeout(timeout);
		};
	}, []);
	return { now };
}
