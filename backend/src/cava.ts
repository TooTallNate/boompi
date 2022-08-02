import ini from 'ini';
import { join } from 'path';
import { writeFile, remove, createReadStream } from 'fs-extra';
import { spawn, spawnSync } from 'child_process';

export interface CavaConfig {
    bars: number;
    bitFormat: 8 | 16;
}

/**
 * Spawn `cava` to output raw binary data.
 * https://github.com/karlstav/cava/issues/123#issuecomment-307891020
 */
export async function startCava(config: CavaConfig) {
    const backendRoot = join(__dirname, '..');
    const configFilePath = join(backendRoot, 'cava.config');
    const fifoPath = join(backendRoot, 'cava.fifo');
    const configFile = ini.stringify({
        general: {
            bars: config.bars
        },
        output: {
            method: 'raw',
            raw_target: fifoPath,
            bit_format: `${config.bitFormat}bit`
        }
    });

    await remove(fifoPath);
    spawnSync('mkfifo', [fifoPath]);
    const fifo = createReadStream(fifoPath);

    await writeFile(configFilePath, configFile);
    const proc = spawn('cava', ['-p', configFilePath], {
        cwd: backendRoot,
        stdio: ['ignore', 'inherit', 'inherit']
    });
    return { proc, fifo };
}