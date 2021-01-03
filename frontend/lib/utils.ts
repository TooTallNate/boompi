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
	if (dataMin < 0) {
		return dataMin - (100 + (dataMin % 100));
	}
	return dataMin + (100 - (dataMin % 100)) - 100;
}

export function calculateAmpsMaxDomain(dataMax: number): number {
	if (dataMax < 0) {
		return dataMax - (dataMax % 100);
	}
	return dataMax + (100 - (dataMax % 100));
}

assert.equal(formatAmps(23), '23mA');
assert.equal(formatAmps(123), '123mA');
assert.equal(formatAmps(999), '999mA');
assert.equal(formatAmps(1000), '1.0A');
assert.equal(formatAmps(1349), '1.3A');
assert.equal(formatAmps(1351), '1.4A');
assert.equal(formatAmps(1999), '2.0A');

assert.equal(calculateAmpsMinDomain(1), 0);
assert.equal(calculateAmpsMinDomain(100), 100);
assert.equal(calculateAmpsMinDomain(53), 0);
assert.equal(calculateAmpsMinDomain(-93), -100);
assert.equal(calculateAmpsMinDomain(-123), -200);

assert.equal(calculateAmpsMaxDomain(1), 100);
assert.equal(calculateAmpsMaxDomain(100), 200);
assert.equal(calculateAmpsMaxDomain(53), 100);
assert.equal(calculateAmpsMaxDomain(-93), 0);
assert.equal(calculateAmpsMaxDomain(-123), -100);
