//! Driver for the TI INA260 precision current/voltage/power monitor.
//!
//! Rust port of the v1 `@tootallnate/ina260` Node.js driver, written against
//! the [`embedded_hal::i2c::I2c`] trait so it is platform independent. On the
//! boombox, `boompid` provides a Linux I2C bus (`linux-embedded-hal`) whose
//! bus number comes from per-device config (the Pi 3 box reaches the chip via
//! the HyperPixel's bit-banged auxiliary bus, not bus 1).
//!
//! Register map (all registers are 16-bit big-endian):
//!
//! | Reg  | Name            | LSB      |
//! |------|-----------------|----------|
//! | 0x00 | Configuration   | -        |
//! | 0x01 | Current         | 1.25 mA  |
//! | 0x02 | Bus Voltage     | 1.25 mV  |
//! | 0x03 | Power           | 10 mW    |
//! | 0x06 | Mask/Enable     | -        |
//! | 0x07 | Alert Limit     | -        |
//! | 0xFE | Manufacturer ID | ("TI")   |
//! | 0xFF | Die ID          | -        |

#![cfg_attr(not(test), no_std)]

use embedded_hal::i2c::I2c;

/// Default I2C address (A0/A1 tied to GND).
pub const DEFAULT_ADDRESS: u8 = 0x40;

/// Expected value of the manufacturer ID register ("TI").
pub const MANUFACTURER_ID: u16 = 0x5449;

/// Configuration used by Boompi v1: 16-sample averaging, 140 µs bus-voltage
/// conversion, 1.1 ms shunt-current conversion, continuous mode.
pub const V1_CONFIG: u16 = 0x4427;

/// Register addresses.
pub mod reg {
    pub const CONFIG: u8 = 0x00;
    pub const CURRENT: u8 = 0x01;
    pub const BUS_VOLTAGE: u8 = 0x02;
    pub const POWER: u8 = 0x03;
    pub const MASK_ENABLE: u8 = 0x06;
    pub const ALERT_LIMIT: u8 = 0x07;
    pub const MANUFACTURER_ID: u8 = 0xFE;
    pub const DIE_ID: u8 = 0xFF;
}

/// Volts per LSB of the bus voltage register.
pub const VOLTAGE_LSB_V: f64 = 0.001_25;
/// Amps per LSB of the (signed) current register.
pub const CURRENT_LSB_A: f64 = 0.001_25;
/// Watts per LSB of the power register.
pub const POWER_LSB_W: f64 = 0.01;

/// An INA260 on an I2C bus.
pub struct Ina260<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ina260<I2C> {
    pub fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// Release the underlying I2C bus.
    pub fn release(self) -> I2C {
        self.i2c
    }

    pub fn read_register(&mut self, register: u8) -> Result<u16, I2C::Error> {
        let mut buf = [0u8; 2];
        self.i2c.write_read(self.address, &[register], &mut buf)?;
        Ok(u16::from_be_bytes(buf))
    }

    pub fn write_register(&mut self, register: u8, value: u16) -> Result<(), I2C::Error> {
        let bytes = value.to_be_bytes();
        self.i2c
            .write(self.address, &[register, bytes[0], bytes[1]])
    }

    pub fn read_config(&mut self) -> Result<u16, I2C::Error> {
        self.read_register(reg::CONFIG)
    }

    pub fn write_config(&mut self, config: u16) -> Result<(), I2C::Error> {
        self.write_register(reg::CONFIG, config)
    }

    /// Bus voltage in volts. Always non-negative.
    pub fn voltage(&mut self) -> Result<f64, I2C::Error> {
        Ok(self.read_register(reg::BUS_VOLTAGE)? as f64 * VOLTAGE_LSB_V)
    }

    /// Current in amps. Negative while the battery is charging
    /// (current flowing into the pack).
    pub fn current(&mut self) -> Result<f64, I2C::Error> {
        Ok(self.read_register(reg::CURRENT)? as i16 as f64 * CURRENT_LSB_A)
    }

    /// Power in watts.
    pub fn power(&mut self) -> Result<f64, I2C::Error> {
        Ok(self.read_register(reg::POWER)? as f64 * POWER_LSB_W)
    }

    /// Reads the manufacturer ID register; a healthy chip returns
    /// [`MANUFACTURER_ID`]. Useful as a presence/sanity check at startup.
    pub fn manufacturer_id(&mut self) -> Result<u16, I2C::Error> {
        self.read_register(reg::MANUFACTURER_ID)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};

    /// Minimal register-file I2C mock: a 1-byte write selects the register
    /// pointer, a 3-byte write sets a register, a 2-byte read returns the
    /// selected register big-endian.
    struct MockBus {
        regs: [u16; 256],
        pointer: u8,
    }

    impl MockBus {
        fn new() -> Self {
            Self {
                regs: [0; 256],
                pointer: 0,
            }
        }
    }

    impl ErrorType for MockBus {
        type Error = ErrorKind;
    }

    impl I2c for MockBus {
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for op in operations {
                match op {
                    Operation::Write(bytes) => match bytes {
                        [reg] => self.pointer = *reg,
                        [reg, hi, lo] => {
                            self.pointer = *reg;
                            self.regs[*reg as usize] = u16::from_be_bytes([*hi, *lo]);
                        }
                        _ => return Err(ErrorKind::Other),
                    },
                    Operation::Read(buf) => {
                        if buf.len() != 2 {
                            return Err(ErrorKind::Other);
                        }
                        buf.copy_from_slice(&self.regs[self.pointer as usize].to_be_bytes());
                    }
                }
            }
            Ok(())
        }
    }

    fn device_with(reg_addr: u8, value: u16) -> Ina260<MockBus> {
        let mut bus = MockBus::new();
        bus.regs[reg_addr as usize] = value;
        Ina260::new(bus, DEFAULT_ADDRESS)
    }

    #[test]
    fn voltage_scaling() {
        // 20.0 V = 16000 * 1.25 mV
        let mut dev = device_with(reg::BUS_VOLTAGE, 16_000);
        assert!((dev.voltage().unwrap() - 20.0).abs() < 1e-9);
    }

    #[test]
    fn current_is_signed() {
        // -1 A = -800 * 1.25 mA (two's complement)
        let mut dev = device_with(reg::CURRENT, (-800i16) as u16);
        assert!((dev.current().unwrap() + 1.0).abs() < 1e-9);
    }

    #[test]
    fn power_scaling() {
        // 12.34 W = 1234 * 10 mW
        let mut dev = device_with(reg::POWER, 1234);
        assert!((dev.power().unwrap() - 12.34).abs() < 1e-9);
    }

    #[test]
    fn config_round_trip() {
        let mut dev = Ina260::new(MockBus::new(), DEFAULT_ADDRESS);
        dev.write_config(V1_CONFIG).unwrap();
        assert_eq!(dev.read_config().unwrap(), V1_CONFIG);
    }

    #[test]
    fn manufacturer_id_check() {
        let mut dev = device_with(reg::MANUFACTURER_ID, MANUFACTURER_ID);
        assert_eq!(dev.manufacturer_id().unwrap(), MANUFACTURER_ID);
    }
}
