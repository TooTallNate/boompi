import { useEffect } from 'react';

export default function useRequestAnimationFrame(fn: () => void, deps?: any[]) {
	return useEffect(() => {
		const raf = window.requestAnimationFrame(fn);
		return () => {
			window.cancelAnimationFrame(raf);
		};
	}, deps);
}
