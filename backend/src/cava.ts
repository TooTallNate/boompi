import ini from 'ini';
import { join } from 'path';
import { writeFile } from 'fs-extra';
import { ChildProcess, spawn } from 'child_process';

export interface CavaConfig {
    bars: number;
    bitFormat: 8 | 16;
}

/**
 * Spawn `cava` to output raw binary data.
 * https://github.com/karlstav/cava/issues/123#issuecomment-307891020
 */
export async function startCava(config: CavaConfig): Promise<ChildProcess> {
    const configFilePath = join(__dirname, '..', 'cava.config');
    const configFile = ini.stringify({
        general: {
            bars: config.bars
        },
        output: {
            method: 'raw',
            raw_target: '/dev/stdout',
            bit_format: `${config.bitFormat}bit`
        }
    });
    await writeFile(configFilePath, configFile);
    const proc = spawn('cava', ['-p', configFilePath], {
        stdio: ['ignore', 'pipe', 'inherit']
    });
    return proc;
}