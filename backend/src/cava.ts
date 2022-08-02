import ini from 'ini';
import { join } from 'path';
import { writeFile } from 'fs-extra';
import { spawn } from 'child_process';

export interface CavaConfig {
	bars: number;
	bitFormat: 8 | 16;
	framerate: number;
}

/**
 * Spawn `cava` to output raw binary data.
 * https://github.com/karlstav/cava/issues/123#issuecomment-307891020
 */
export async function startCava(config: CavaConfig) {
	const backendRoot = join(__dirname, '..');
	const configFilePath = join(backendRoot, 'cava.config');
	const configFile = ini.stringify({
		general: {
			framerate: config.framerate,
			bars: config.bars,
		},
		output: {
			method: 'raw',
			bit_format: `${config.bitFormat}bit`,

			// Requires custom `cava` fork:
			// https://github.com/TooTallNate/cava/tree/add/raw_target_fd
			raw_target_fd: 3,
		},
	});

	await writeFile(configFilePath, configFile);
	const proc = spawn('cava', ['-p', configFilePath], {
		cwd: backendRoot,
		stdio: ['ignore', 'inherit', 'inherit', 'pipe'],
	});
	const stream = proc.stdio[3];
	if (!stream) {
		throw new Error('Could not get FD 3 for cava');
	}
	return {
		proc,
		stream,
		bars: config.bars,
		pageSize: config.bars * (config.bitFormat / 8),
	};
}
