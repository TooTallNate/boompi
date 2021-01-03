const assert = require('assert');

// Adapted from: https://stackoverflow.com/a/21294619/376773
export function millisToMinutesAndSeconds(millis: number): string {
	const minutes = Math.floor(millis / 60000);
	const seconds = Math.floor((millis % 60000) / 1000);
	const parts = [];
	if (minutes > 0) {
		parts.push(minutes, 'm');
	}
	if (seconds > 0 || minutes === 0) {
		parts.push(seconds, 's');
	}
	parts.push(' ago');
	return parts.join('');
}

export function formatAmps(current: number): string {
	return current < 1000 && current > -1000
		? `${current}mA`
		: `${(current / 1000).toFixed(1)}A`;
}

export function calculateAmpsMinDomain(dataMin: number): number {
	const min = dataMin + (100 - (dataMin % 100)) - 100;
	//const min = dataMin + ((dataMin % 100) - 100);
	//console.log({ dataMin, min });
	return min;
	//return Math.min(min, 0);
}

export function calculateAmpsMaxDomain(dataMax: number): number {
	const max = dataMax + (100 - (dataMax % 100));
	//console.log({ dataMax, max });
	return max;
	//return Math.max(0, max);
}

assert.equal(calculateAmpsMaxDomain(1), 100);
