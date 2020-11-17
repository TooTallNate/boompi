import { useEffect, useState } from 'react';

export default function useNow() {
	const [now, setNow] = useState(new Date());
	useEffect(() => {
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
			clearTimeout(timeout);
		};
	}, []);
	return { now };
}
