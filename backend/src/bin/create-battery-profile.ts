import fs from 'fs-extra';
import i2c from 'i2c-bus';
import { INA260 } from '@tootallnate/ina260';

const CSV_FILE = 'battery.csv';
const TIMEOUT_MS = 60 * 1000; /* 1 minute */

async function main() {
	const i2cBus = i2c.openSync(1);
	const ina = new INA260(i2cBus, 0x40);

	const header = `Date,Voltage,Current,Power\n`;
	await fs.writeFile(CSV_FILE, header);

	while (true) {
		const [voltage, current, power] = await Promise.all([
			ina.readVoltage(),
			ina.readCurrent(),
			ina.readPower(),
		]);

		const line =
			[new Date().toISOString(), voltage, current, power].join(',') +
			'\n';

		await Promise.all([
			new Promise((r) => setTimeout(r, TIMEOUT_MS)),
			fs.appendFile(CSV_FILE, line),
		]);
	}
}

main().catch((err) => {
	console.error(err);
	process.exit(1);
});
