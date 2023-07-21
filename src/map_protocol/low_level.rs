use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use super::MapError;

const BUFFER_SIZE: u16 = 560;

#[repr(u8)]
#[derive(PartialEq, Eq, Copy, Clone)]
pub enum LowLevelCommands {
    ToRead = 0x72,
    ToWrite = 0x77,
}
impl From<LowLevelCommands> for u8 {
    fn from(value: LowLevelCommands) -> Self {
        value as u8
    }
}
#[derive(Debug)]
pub struct LowLevelProtocol {
    file: File,
    sum: u8,
    pub buffer: [u8; BUFFER_SIZE as usize],
    pub last_read_bytes_index: usize,
}
impl LowLevelProtocol {
    pub fn new(file: File) -> Self {
        Self {
            file,
            sum: 0,
            buffer: [0; BUFFER_SIZE as usize],
            last_read_bytes_index: 0,
        }
    }
    pub fn clear_buffer(&mut self) {
        self.buffer.fill(0);
        self.last_read_bytes_index = 0;
    }

    pub fn get_actually_read_slice(&self) -> &[u8] {
        &self.buffer[1..=self.last_read_bytes_index]
    }

    fn put_char(&mut self, c: u8) -> Result<(), MapError> {
        let mut char1: [u8; 1] = [0; 1];
        let mut counter = 0;

        if self.file.write(&[c]).map_err(MapError::IOError)? != 1 {
            return Err(MapError::WriteError);
        }
        loop {
            if self.file.read(&mut char1).map_err(MapError::IOError)? != 1 {
                return Err(MapError::WriteError);
            }

            if c == char1[0] {
                break;
            }
            counter += 1;
            if counter > 20 {
                return Err(MapError::WriteError);
            }
        }

        Ok(())
    }

    fn code_db(&mut self, a: u8) -> Result<(), MapError> {
        if a == (b'\n') {
            self.sum += 0xDB;
            self.put_char(0xDB)?;
            self.sum += 0xDC;
            self.put_char(0xDC)?;
        } else if a == 0xDB {
            self.sum += 0xDB;
            self.put_char(0xDB)?;

            self.sum += 0xDD;
            self.put_char(0xDD)?
        } else {
            self.sum += a;
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
        let mut a: [u8; 4] = [0; 4];
        self.sum = 0;
        a[0] = command.into();
        a[1] = (page & 0xFF) as u8;
        a[2] = (addr >> 8) as u8;
        a[3] = (addr & 0xFF) as u8;
        assert!(page < BUFFER_SIZE);
        for item in &a {
            self.code_db(*item)?;
        }
        if command == LowLevelCommands::ToWrite {
            for i in 0..=page {
                self.code_db(self.buffer[i as usize])?;
            }
        }
        self.sum = 0xFF - self.sum;
        self.sum += 1;
        self.put_char(self.sum)?;
        if self.sum != b'\n' {
            self.put_char(b'\n')?;
        }

        Ok(())
    }
    pub fn read_answer(&mut self) -> Result<(), MapError> {
        self.file
            .read_exact(&mut self.buffer[0..0])
            .map_err(MapError::IOError)?;
        let mut bytes_read: usize = 0;

        loop {
            self.file
                .write_all(&self.buffer[bytes_read..bytes_read])
                .map_err(MapError::IOError)?;
            bytes_read += 1;
            self.file
                .read_exact(&mut self.buffer[bytes_read..bytes_read])
                .map_err(MapError::IOError)?;

            if self.buffer[bytes_read] == 0x0A || bytes_read >= self.buffer.len() - 1 {
                break;
            }
        }
        self.file
            .write_all(&self.buffer[bytes_read..bytes_read])
            .map_err(MapError::IOError)?;

        if self.buffer[0] == 0x65 {
            return Err(MapError::FirstByteis65DontKnowWhatItMeans);
        }
        if self.buffer[0] != 0x6f {
            return Err(MapError::UnknownError(self.buffer[0]));
        }

        let mut sum_r: u8 = 0;

        for byte in self.buffer[0..bytes_read].iter() {
            sum_r += byte;
        }
        sum_r = 0xff - sum_r;
        sum_r += 1;
        if sum_r == 0x0a && self.buffer[bytes_read] != 0x0a {
            return Err(MapError::ChecksumFailed(sum_r));
        }
        if sum_r != 0x0a && sum_r != self.buffer[bytes_read - 1] {
            return Err(MapError::ChecksumFailed(sum_r));
        }
        self.last_read_bytes_index = bytes_read;
        self.decode_answer();

        Ok(())
    }

    fn decode_answer(&mut self) {
        let mut idx = 0;

        loop {
            if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDC {
                self.buffer[idx] = 0x0A;
                self.buffer[idx + 1..self.last_read_bytes_index].rotate_left(0);
                self.last_read_bytes_index -= 1;
            } else if self.buffer[idx] == 0xDB && self.buffer[idx + 1] == 0xDD {
                // self.buffer[c] = 0xDB;
                self.buffer[idx + 1..self.last_read_bytes_index].rotate_left(0);
                self.last_read_bytes_index -= 1;
            }
            idx += 1;
            if idx >= self.last_read_bytes_index {
                break;
            }
        }
    }
}
