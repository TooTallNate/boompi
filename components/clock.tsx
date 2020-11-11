import format from 'dateformat';
import useNow from '@lib/use-now';

interface ClockProps {
	date: Date;
}

export default function Clock({ date }) {
	let formatted = format(date, 'HH:MM:ss');
	if (date.getSeconds() % 2 === 0) {
		formatted = formatted.replace(/\:/g, ' ');
	}
	return <span style={{ fontFamily: 'monospace' }}>{formatted}</span>;
}
