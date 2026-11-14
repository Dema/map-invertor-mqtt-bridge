use std::{
    fmt::Display,
    io::{Read, Write},
};

use serialport::SerialPort;

use super::MapError;

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
/// One decoded answer from the inverter.
///
/// `data` is the payload alone: the `0x6f` status byte, the checksum, the
/// `0x0a` terminator and every `0xdb` escape sequence have been removed, so
/// `data.len()` is the decoded payload length.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct LowLevelProtocol {
    port: Box<dyn SerialPort>,
    sum: u8,
    /// Scratch space for one response: status byte, escaped payload, checksum,
    /// terminator. Never leaves this type.
    buffer: [u8; BUFFER_SIZE as usize],
}
impl LowLevelProtocol {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self {
            port,
            sum: 0,
            buffer: [0; BUFFER_SIZE as usize],
        }
    }

    fn clear_buffer(&mut self) {
        self.buffer.fill(0);
    }

    fn put_char(&mut self, c: u8) -> Result<(), MapError> {
        let mut verify_buffer: [u8; 1] = [0; 1];
        let mut counter: u8 = 0;

        if self.port.write(&[c])? != verify_buffer.len() {
            return Err(MapError::WriteError);
        }
        loop {
            {
                let bytes_read = self.port.read(&mut verify_buffer)?;
                if bytes_read != 1 {
                    return Err(MapError::VerifyReadAfterWriteError { count: bytes_read });
                }
            }

            if c == verify_buffer[0] {
                break;
            }
            counter += 1;
            if counter > 20 {
                return Err(MapError::VerifyReadAfterWriteRunawayError);
            }
        }

        Ok(())
    }

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
        Ok(())
    }

    pub fn send_command_clean_buffer(
        &mut self,
        command: LowLevelCommands,
        addr: u16,
        page: u16,
    ) -> Result<(), MapError> {
        self.clear_buffer();
        self.send_command(command, addr, page)
    }

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

    /// Performs the byte-echo handshake that reads one answer and returns its
    /// decoded payload.
    ///
    /// The inverter echoes every byte it sends, so each received byte is written
    /// back before the next one is read. The raw response is `0x6f` (status),
    /// payload, checksum and `0x0a` terminator, with `0x0a` and `0xdb` bytes
    /// escaped as `0xdb 0xdc` / `0xdb 0xdd`; this validates the checksum over the
    /// escaped bytes and unescapes the payload.
    pub fn exchange(&mut self) -> Result<Answer, MapError> {
        // Unescaping happens in place and looks one byte past the last payload
        // byte, so the tail of the buffer has to stay zeroed.
        self.buffer.fill(0);

        self.port.read_exact(&mut self.buffer[0..=0])?;

        let mut idx: usize = 0;

        loop {
            self.port
                .write_all(&self.buffer[idx..=idx])?;

            idx += 1;
            self.port
                .read_exact(&mut self.buffer[idx..=idx])?;

            if self.buffer[idx] == 0x0A || idx >= self.buffer.len() - 1 {
                break;
            }
        }
        self.port
            .write_all(&self.buffer[idx..=idx])?;

        if self.buffer[0] == 0x65 {
            return Err(MapError::FirstByteis65DontKnowWhatItMeans);
        }
        if self.buffer[0] != 0x6f {
            return Err(MapError::UnknownValueError {
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
            });
        }
        if sum_r != 0x0a && sum_r != self.buffer[idx - 1] {
            return Err(MapError::ChecksumFailed {
                value: sum_r,
            });
        }
        let last_read_byte = self.decode_answer(idx);

        // Not payload: the checksum sits just before the terminator, and when the
        // checksum is itself `0x0a` it is the byte the read loop stopped on and no
        // separate terminator was sent. Either way the framing stays in `buffer`.
        let framing_bytes = if sum_r == 0x0a { 1 } else { 2 };
        let data_len = last_read_byte.saturating_sub(framing_bytes);

        Ok(Answer {
            data: self.buffer[1..1 + data_len].to_vec(),
        })
    }

    /// Unescapes `buffer[1..=last]` in place and returns the index of the last
    /// byte the read put there after every `0xdb` escape pair has been collapsed.
    fn decode_answer(&mut self, mut last: usize) -> usize {
        let mut idx = 1;

        while idx <= last {
            if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDC {
                self.buffer[idx] = 0x0A;
                last -= 1;
                for i in idx..last - 1 {
                    self.buffer[i + 1] = self.buffer[i + 2];
                }
            } else if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDD {
                self.buffer[idx] = 0xDB;
                last -= 1;
                for i in idx..last - 1 {
                    self.buffer[i + 1] = self.buffer[i + 2];
                }
            }
            idx += 1;
        }
        last
    }
}
