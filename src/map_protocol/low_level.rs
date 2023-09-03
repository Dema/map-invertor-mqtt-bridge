use std::{
    fmt::Display,
    io::{Read, Write},
};

use serialport::SerialPort;
use snafu::ResultExt;
use tracing::{instrument, trace};

use super::{IOSnafu, MapError};

const BUFFER_SIZE: u16 = 560;

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum LowLevelCommands {
    ToRead = 0x72,
    ToWrite = 0x77,
}
impl From<LowLevelCommands> for u8 {
    fn from(value: LowLevelCommands) -> Self {
        value as u8
    }
}
impl Display for LowLevelCommands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowLevelCommands::ToRead => write!(f, "ToRead"),
            LowLevelCommands::ToWrite => write!(f, "ToWrite"),
        }
    }
}
#[derive(Debug)]
pub struct LowLevelProtocol {
    port: Box<dyn SerialPort>,
    sum: u8,
    pub buffer: [u8; BUFFER_SIZE as usize],
    pub last_read_bytes_index: usize,
}
impl LowLevelProtocol {
    #[instrument]
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self {
            port,
            sum: 0,
            buffer: [0; BUFFER_SIZE as usize],
            last_read_bytes_index: 0,
        }
    }
    #[instrument(skip(self))]
    pub fn clear_buffer(&mut self) {
        self.buffer.fill(0);
        self.last_read_bytes_index = 0;
    }
    #[instrument(skip(self))]
    pub fn get_actually_read_slice(&self) -> &[u8] {
        &self.buffer[1..=self.last_read_bytes_index]
    }
    #[instrument(skip(self))]
    fn put_char(&mut self, c: u8) -> Result<(), MapError> {
        let mut verify_buffer: [u8; 1] = [0; 1];
        let mut counter: u8 = 0;

        if self.port.write(&[c]).context(IOSnafu)? != verify_buffer.len() {
            return Err(MapError::WriteError {
                backtrace: std::backtrace::Backtrace::capture(),
            });
        }
        loop {
            {
                let bytes_read = self.port.read(&mut verify_buffer).context(IOSnafu)?;
                if bytes_read != 1 {
                    return Err(MapError::VerifyReadAfterWriteError {
                        backtrace: std::backtrace::Backtrace::capture(),
                        count: bytes_read,
                    });
                }
            }

            if c == verify_buffer[0] {
                break;
            }
            counter += 1;
            if counter > 20 {
                return Err(MapError::VerifyReadAfterWriteRunawayError {
                    backtrace: std::backtrace::Backtrace::capture(),
                });
            }
        }

        Ok(())
    }
    #[instrument(skip(self))]
    fn code_db(&mut self, a: u8) -> Result<(), MapError> {
        if a == (b'\n') {
            self.sum = self.sum.wrapping_add(0xDB);
            self.put_char(0xDB)?;
            self.sum = self.sum.wrapping_add(0xDC);
            self.put_char(0xDC)?;
        } else if a == 0xDB {
            self.sum = self.sum.wrapping_add(0xDB);
            self.put_char(0xDB)?;

            self.sum = self.sum.wrapping_add(0xDD);
            self.put_char(0xDD)?
        } else {
            self.sum = self.sum.wrapping_add(a);
            self.put_char(a)?;
        }
        tracing::trace!("sum: {}", self.sum);
        Ok(())
    }
    #[instrument(skip(self))]
    pub fn send_command_clean_buffer(
        &mut self,
        command: LowLevelCommands,
        addr: u16,
        page: u16,
    ) -> Result<(), MapError> {
        self.clear_buffer();
        self.send_command(command, addr, page)
    }

    #[instrument(skip(self))]
    pub fn send_command(
        &mut self,
        command: LowLevelCommands,
        addr: u16,
        page: u16,
    ) -> Result<(), MapError> {
        let mut data: [u8; 4] = [0; 4];
        self.sum = 0;
        data[0] = command.into();
        data[1] = (page & 0xFF) as u8;
        data[2] = (addr >> 8) as u8;
        data[3] = (addr & 0xFF) as u8;
        assert!(page < BUFFER_SIZE);
        trace!("data: {:?}", data);
        for item in &data {
            self.code_db(*item)?;
        }
        if command == LowLevelCommands::ToWrite {
            for i in 0..=page {
                self.code_db(self.buffer[i as usize])?;
            }
        }
        self.sum = 0xFF - self.sum;
        self.sum = self.sum.wrapping_add(1);
        self.put_char(self.sum)?;
        if self.sum != b'\n' {
            self.put_char(b'\n')?;
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn read_answer(&mut self) -> Result<(), MapError> {
        self.port
            .read_exact(&mut self.buffer[0..=0])
            .context(IOSnafu)?;

        let mut idx: usize = 0;

        loop {
            self.port
                .write_all(&self.buffer[idx..=idx])
                .context(IOSnafu)?;

            idx += 1;
            self.port
                .read_exact(&mut self.buffer[idx..=idx])
                .context(IOSnafu)?;

            if self.buffer[idx] == 0x0A || idx >= self.buffer.len() - 1 {
                break;
            }
        }
        self.port
            .write_all(&self.buffer[idx..=idx])
            .context(IOSnafu)?;

        if self.buffer[0] == 0x65 {
            return Err(MapError::FirstByteis65DontKnowWhatItMeans {
                backtrace: std::backtrace::Backtrace::capture(),
            });
        }
        if self.buffer[0] != 0x6f {
            return Err(MapError::UnknownValueError {
                backtrace: std::backtrace::Backtrace::capture(),
                value: (self.buffer[0]),
            });
        }

        let mut sum_r: u8 = 0;
        // let mut cnt = 0;
        for byte in self.buffer[0..idx - 1].iter() {
            sum_r = sum_r.wrapping_add(*byte);
            // cnt += 1;
        }
        // dbg!(cnt);
        // println!("sum_r1={}", sum_r);
        sum_r = 0xff - sum_r;
        // println!("sum_r2={}", sum_r);
        sum_r = sum_r.wrapping_add(1);
        // println!("sum_r3={}", sum_r);
        if sum_r == 0x0a && self.buffer[idx] != 0x0a {
            return Err(MapError::ChecksumFailed {
                value: sum_r,
                backtrace: std::backtrace::Backtrace::capture(),
            });
        }
        if sum_r != 0x0a && sum_r != self.buffer[idx - 1] {
            return Err(MapError::ChecksumFailed {
                value: sum_r,
                backtrace: std::backtrace::Backtrace::capture(),
            });
        }
        self.last_read_bytes_index = idx;
        self.decode_answer();

        Ok(())
    }
    #[instrument(skip(self))]
    fn decode_answer(&mut self) {
        let mut idx = 1;

        while idx <= self.last_read_bytes_index {
            {
                if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDC {
                    self.buffer[idx] = 0x0A;
                    self.last_read_bytes_index -= 1;
                    // if idx + 2 <= self.last_read_bytes_index {
                    //     self.buffer.copy_within((idx + 2)..=self.last_read_bytes_index , idx + 1);
                    // }
                    for i in idx..self.last_read_bytes_index - 1 {
                        self.buffer[i + 1] = self.buffer[i + 2];
                    }
                } else if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDD {
                    self.buffer[idx] = 0xDB;

                    self.last_read_bytes_index -= 1;
                    for i in idx..self.last_read_bytes_index - 1 {
                        self.buffer[i + 1] = self.buffer[i + 2];
                    }
                }
                idx += 1;
            }
        }
    }
}
