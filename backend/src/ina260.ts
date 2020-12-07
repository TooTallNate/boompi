/**
 * Node.js module to read values from the INA260 bi-directional
 * current and power monitor.
 *
 * Reference: https://github.com/adafruit/Adafruit_CircuitPython_INA260/blob/master/adafruit_ina260.py
 *
 * Copyright (c) 2020 Nathan Rajlich <n@n8.io> (https://n8.io/)
 * Copyright (c) 2017-2020 Peter Müller <peter@crycode.de> (https://crycode.de/)
 */

import { I2CBus } from 'i2c-bus';

/**
 * Address of the Configuration Register.
 */
export const CONFIGURATION_REGISTER = 0x00;

/**
 * Address of the Shunt Voltage Register.
 */
export const SHUNT_VOLTAGE_REGISTER = 0x01;

/**
 * Address of the Bus Voltage Register.
 */
export const BUS_VOLTAGE_REGISTER = 0x02;

/**
 * Address of the Power Register.
 */
export const POWER_REGISTER = 0x03;

/**
 * Address of the Current Register.
 */
export const CURRENT_REGISTER = 0x04;

/**
 * Address of the Calibration Register.
 */
export const CALIBRATION_REGISTER = 0x05;

/**
 * Address of the Mask/Enable Register.
 */
export const MASK_ENABLE_REGISTER = 0x06;

/**
 * Address of the Alert Limit Register.
 */
export const ALERT_LIMIT_REGISTER = 0x07;

/**
 * Address of the Manufactor ID Register.
 */
export const MANUFACTOR_ID_REGISTER = 0xfe;

/**
 * Address of the Die ID Register.
 */
export const DIE_ID_REGISTER = 0xff;

/**
 * Class for the power monitor INA260.
 * @param  {I2cBus}     i2cBus  Instance of an opened i2c-bus.
 * @param  {number}     address The address of the INA260 IC.
 */
export class INA260 {
	/**
	 * Instance of the used i2c-bus object.
	 * @type {I2cBus}
	 */
	private _i2cBus: I2CBus;

	/**
	 * The address of the INA260 IC.
	 * @type {number}
	 */
	private _address: number;

	/**
	 * Constructor for the power monitor INA260.
	 * @param  {I2cBus} i2cBus  Instance of an opened i2c-bus.
	 * @param  {number} address The address of the INA260 IC.
	 */
	constructor(i2cBus: I2CBus, address: number) {
		this._i2cBus = i2cBus;
		this._address = address;
	}

	/**
	 * Writes a value to a specific register.
	 * Returns a Promise which will be resolves if the value is written, or rejected in case of an error.
	 * @param  {number}  register The register address.
	 * @param  {number}  value    The value. Should be 16bit integer.
	 * @return {Promise}
	 */
	public writeRegister(register: number, value: number): Promise<void> {
		const buf = Buffer.alloc(2);
		buf[0] = (value >> 8) & 0xff;
		buf[1] = value & 0xff;

		return new Promise<void>((resolve, reject) => {
			this._i2cBus.writeI2cBlock(
				this._address,
				register,
				2,
				buf,
				(err: any, _bytesWritten: number, _buffer: Buffer) => {
					if (err) {
						reject(err);
					} else {
						resolve();
					}
				}
			);
		});
	}

	/**
	 * Reads a value from a specific register.
	 * Returns a Promise which will be resolved with the read value, or rejected in case of an error.
	 * @param  {number}          register The register address.
	 * @return {Promise<number>}
	 */
	public readRegister(register: number): Promise<number> {
		const buf = Buffer.alloc(2);

		return new Promise<number>(
			(
				resolve: (bytesWritten: number) => void,
				reject: (err: Error) => void
			) => {
				this._i2cBus.readI2cBlock(
					this._address,
					register,
					2,
					buf,
					(err: any, _bytesRead: number, buffer: Buffer) => {
						if (err) {
							reject(err);
						} else {
							resolve(buffer.readInt16BE(0));
						}
					}
				);
			}
		);
	}

	/**
	 * Reads the actual bus voltage in volts (V).
	 */
	async readVoltage(): Promise<number> {
		const busVoltage = await this.readRegister(BUS_VOLTAGE_REGISTER);
		return busVoltage * 0.00125;
	}

	/**
	 * Reads the current (between V+ and V-) in milliamps (mA).
	 */
	async readCurrent(): Promise<number> {
		const shuntVoltage = await this.readRegister(SHUNT_VOLTAGE_REGISTER);
		return shuntVoltage * 1.25;
	}

	/**
	 * Reads the power being delivered to the load in milliwatts (mW).
	 */
	async readPower(): Promise<number> {
		const power = await this.readRegister(POWER_REGISTER);
		return power * 10;
	}
}
