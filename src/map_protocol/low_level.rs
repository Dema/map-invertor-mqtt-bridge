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

/// Escapes `data` for the wire: `0x0a` is sent as `0xdb 0xdc` and `0xdb` as
/// `0xdb 0xdd`, every other byte is sent unchanged.
///
/// The escape is context free, one byte in, one or two bytes out, so the sum of
/// the returned bytes is exactly the sum `code_db` in `mapd.c` accumulates for
/// the command checksum.
fn escape(data: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(data.len());
    for &byte in data {
        match byte {
            0x0A => escaped.extend_from_slice(&[0xDB, 0xDC]),
            0xDB => escaped.extend_from_slice(&[0xDB, 0xDD]),
            _ => escaped.push(byte),
        }
    }
    escaped
}

/// Reverses [`escape`] on one escaped payload.
///
/// `data` must be the payload alone: the `0x6f` status byte, the checksum and
/// the `0x0a` terminator have to be stripped first, because the status byte is
/// never escaped (the C `decode_answer` starts at `Buffer[1]`).
///
/// As in `mapd.c`, `0xdb 0xdc` becomes `0x0a`, `0xdb 0xdd` becomes `0xdb`, and
/// a `0xdb` followed by anything else is left alone. A `0xdb` as the last byte
/// is therefore kept as it is.
///
/// The C version decrements its last index before shifting, so its shift loop
/// copies one byte fewer at the tail. In C's buffer that byte is always
/// framing, which is discarded anyway; only when the checksum itself is `0x0a`
/// does the C tail hold a payload byte, and there C duplicates the last payload
/// byte. Operating on the payload alone, this shifts the whole tail instead.
fn unescape(data: &[u8]) -> Vec<u8> {
    let mut decoded = data.to_vec();
    if decoded.len() < 2 {
        return decoded;
    }

    // Index of the last escaped byte. It shrinks with every collapsed pair, the
    // same way `i` does in `mapd.c`'s `decode_answer`.
    let mut last = decoded.len() - 1;
    let mut idx = 0;
    while idx < last {
        let replacement = match (decoded[idx], decoded[idx + 1]) {
            (0xDB, 0xDC) => Some(0x0A),
            (0xDB, 0xDD) => Some(0xDB),
            _ => None,
        };
        if let Some(byte) = replacement {
            decoded[idx] = byte;
            // Everything after the collapsed pair moves down one slot. This is
            // the C `for (c1 = c; c1 < (i - 1); c1++)` loop, written so that a
            // short payload cannot underflow.
            let tail_end = last;
            last -= 1;
            if idx + 2 <= tail_end {
                decoded.copy_within(idx + 2..=tail_end, idx + 1);
            }
        }
        idx += 1;
    }
    decoded.truncate(last + 1);
    decoded
}

/// The full command frame as it goes on the wire: the four header bytes, the
/// `ToWrite` payload if there is one, all of it escaped, then the checksum and
/// the `0x0a` terminator.
///
/// The checksum is the two's complement of the sum of the escaped bytes, i.e.
/// of exactly the bytes the inverter sees, and a `0x0a` checksum is its own
/// terminator.
fn command_frame(command: LowLevelCommands, addr: u16, page: u16, write_data: &[u8]) -> Vec<u8> {
    let mut data: Vec<u8> = vec![
        command.into(),
        (page & 0xFF) as u8,
        (addr >> 8) as u8,
        (addr & 0xFF) as u8,
    ];
    data.extend_from_slice(write_data);

    let mut frame = escape(&data);
    let sum = frame.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    let checksum = (0xFF - sum).wrapping_add(1);
    frame.push(checksum);
    if checksum != 0x0A {
        frame.push(0x0A);
    }
    frame
}

/// Splits one raw answer into its decoded payload.
///
/// `buffer[0]` is the `0x6f` status byte, `buffer[1..idx]` the escaped payload
/// followed by the checksum, and `buffer[idx]` either the checksum itself when
/// it is `0x0a` (then no terminator was sent) or the `0x0a` terminator.
fn answer_payload(buffer: &[u8], idx: usize, checksum_is_newline: bool) -> Vec<u8> {
    let framing_bytes = if checksum_is_newline { 1 } else { 2 };
    let payload_end = (idx + 1).saturating_sub(framing_bytes).max(1);
    unescape(&buffer[1..payload_end.min(buffer.len())])
}

#[derive(Debug)]
pub struct LowLevelProtocol {
    port: Box<dyn SerialPort>,
    /// Scratch space for one response: status byte, escaped payload, checksum,
    /// terminator. Never leaves this type.
    buffer: [u8; BUFFER_SIZE as usize],
}
impl LowLevelProtocol {
    pub fn new(port: Box<dyn SerialPort>) -> Self {
        Self {
            port,
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
        assert!(page < BUFFER_SIZE);
        let write_data: &[u8] = if command == LowLevelCommands::ToWrite {
            &self.buffer[0..=page as usize]
        } else {
            &[]
        };
        let frame = command_frame(command, addr, page, write_data);

        for byte in frame {
            self.put_char(byte)?;
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
        // Nothing from the previous answer may leak into this one.
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
        // Not payload: the checksum sits just before the terminator, and when the
        // checksum is itself `0x0a` it is the byte the read loop stopped on and no
        // separate terminator was sent.
        Ok(Answer {
            data: answer_payload(&self.buffer, idx, sum_r == 0x0a),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{answer_payload, command_frame, escape, unescape, LowLevelCommands};

    /// The escaped payloads below are the payload halves of buffers that were
    /// run through the C `decode_answer` in `mapd.c`, one per case:
    ///
    /// ```text
    /// 6f 01 db dc 02 55 0a             -> 6f 01 0a 02 55 ...
    /// 6f db dd 05 55 0a                -> 6f db 05 55 ...
    /// 6f 00 db dc db dd 55 55 0a       -> 6f 00 0a db 55 ...
    /// 6f 11 db dc 22 33 55 0a          -> 6f 11 0a 22 33 55 ...
    /// ```
    ///
    /// `0x55` is the checksum and the trailing bytes that the C shift leaves
    /// behind belong to the framing, not the payload.
    #[test]
    fn unescape_matches_the_c_reference_cases() {
        assert_eq!(unescape(&[0x01, 0xDB, 0xDC, 0x02]), vec![0x01, 0x0A, 0x02]);
        assert_eq!(unescape(&[0xDB, 0xDD, 0x05]), vec![0xDB, 0x05]);
        assert_eq!(
            unescape(&[0x00, 0xDB, 0xDC, 0xDB, 0xDD, 0x55]),
            vec![0x00, 0x0A, 0xDB, 0x55]
        );
        assert_eq!(
            unescape(&[0x11, 0xDB, 0xDC, 0x22, 0x33]),
            vec![0x11, 0x0A, 0x22, 0x33]
        );
        // Nothing to unescape: the payload passes through untouched.
        assert_eq!(unescape(&[0x11, 0x22, 0x33]), vec![0x11, 0x22, 0x33]);
    }

    /// The C shift moves every byte after a collapsed pair down one slot and
    /// shortens the payload by one, for each pair, in a single left-to-right
    /// pass. Consecutive pairs therefore need no extra pass over the bytes.
    #[test]
    fn unescape_follows_the_c_shift_on_consecutive_escapes() {
        assert_eq!(
            unescape(&[0xDB, 0xDC, 0xDB, 0xDC, 0xDB, 0xDD]),
            vec![0x0A, 0x0A, 0xDB]
        );
        assert_eq!(
            unescape(&[0xDB, 0xDD, 0xDB, 0xDD, 0xDB, 0xDD]),
            vec![0xDB, 0xDB, 0xDB]
        );
        // A pair in the middle shifts only the bytes behind it.
        assert_eq!(
            unescape(&[0x01, 0x02, 0xDB, 0xDD, 0x03, 0x04, 0x05]),
            vec![0x01, 0x02, 0xDB, 0x03, 0x04, 0x05]
        );
        // A pair as the second to last payload byte.
        assert_eq!(unescape(&[0x01, 0xDB, 0xDC, 0x02]), vec![0x01, 0x0A, 0x02]);
    }

    #[test]
    fn escape_pins_the_wire_bytes() {
        assert_eq!(
            escape(&[0x0A, 0xDB, 0x55]),
            vec![0xDB, 0xDC, 0xDB, 0xDD, 0x55]
        );
        assert_eq!(escape(&[]), Vec::<u8>::new());
        assert_eq!(
            escape(&[0x6F, 0x01, 0x00, 0x10]),
            vec![0x6F, 0x01, 0x00, 0x10]
        );
    }

    /// `send_command` sums the bytes it actually puts on the wire, so the
    /// checksum has to be the sum of the escaped stream, not of the plain bytes.
    #[test]
    fn escaped_byte_sum_is_the_command_checksum_base() {
        let data = [0x72, 0x0A, 0x00, 0xDB];
        let escaped = escape(&data);
        let sum: u32 = escaped.iter().map(|byte| u32::from(*byte)).sum();
        // The escaped stream: 0x72, 0xdb 0xdc, 0x00, 0xdb 0xdd.
        assert_eq!(sum, 0x72 + 0xDB + 0xDC + 0xDB + 0xDD);
        // What `code_db` used to add per input byte: 0x0a costs 0xdb + 0xdc,
        // 0xdb costs 0xdb + 0xdd, everything else costs itself.
        let per_byte: u32 = data.iter().map(|byte| u32::from(*byte)).sum();
        assert_eq!(sum, per_byte + (0xDB + 0xDC - 0x0A) + (0xDB + 0xDD - 0xDB));
    }

    /// A leading or trailing `0xdb`, a pair against the end of the payload, and
    /// 1-2 byte payloads used to underflow `last - 1` and panic.
    #[test]
    fn unescape_does_not_panic_on_minimal_payloads() {
        assert_eq!(unescape(&[]), Vec::<u8>::new());
        assert_eq!(unescape(&[0x0A]), vec![0x0A]);
        assert_eq!(unescape(&[0xDB]), vec![0xDB]);
        assert_eq!(unescape(&[0xDB, 0xDC]), vec![0x0A]);
        assert_eq!(unescape(&[0xDB, 0xDD]), vec![0xDB]);
        assert_eq!(unescape(&[0x01, 0xDB]), vec![0x01, 0xDB]);
        assert_eq!(unescape(&[0x01, 0xDB, 0xDC]), vec![0x01, 0x0A]);
        assert_eq!(unescape(&[0xDB, 0xDC, 0x01]), vec![0x0A, 0x01]);

        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let _ = unescape(&[first, second]);
            }
        }
    }

    /// Anything after `0xdb` other than `0xdc`/`0xdd` is not an escape and is
    /// kept as it is, like the C `else if` chain that simply falls through.
    #[test]
    fn unescape_keeps_malformed_escapes() {
        assert_eq!(unescape(&[0xDB, 0x00]), vec![0xDB, 0x00]);
        assert_eq!(unescape(&[0xDB, 0xDB]), vec![0xDB, 0xDB]);
        assert_eq!(unescape(&[0x01, 0xDB, 0x0A]), vec![0x01, 0xDB, 0x0A]);
        // The first `0xdb` is followed by another `0xdb`, so only the second one
        // starts a pair; the C chain behaves the same way.
        assert_eq!(unescape(&[0xDB, 0xDB, 0xDC]), vec![0xDB, 0x0A]);
    }

    #[test]
    fn escape_unescape_round_trip() {
        assert_eq!(
            unescape(&escape(&[0x0A, 0xDB, 0x55, 0x00, 0xDB, 0x0A])),
            vec![0x0A, 0xDB, 0x55, 0x00, 0xDB, 0x0A]
        );
        for byte in 0..=u8::MAX {
            assert_eq!(unescape(&escape(&[byte])), vec![byte], "byte {byte:#04x}");
        }
        for first in 0..=u8::MAX {
            for second in 0..=u8::MAX {
                let data = [first, second];
                assert_eq!(unescape(&escape(&data)), data, "bytes {data:02x?}");
            }
        }
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        for _ in 0..5000 {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let len = (state >> 33) as usize % 32;
            let data: Vec<u8> = (0..len)
                .map(|i| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let byte = (state >> 33) as u8;
                    // Bias towards the two escaped bytes.
                    if i % 3 == 0 {
                        if i % 2 == 0 {
                            0x0A
                        } else {
                            0xDB
                        }
                    } else {
                        byte
                    }
                })
                .collect();
            assert_eq!(unescape(&escape(&data)), data, "input {data:02x?}");
        }
    }

    /// The whole command frame, not just the escaped payload: header order,
    /// payload inclusion, checksum over the escaped bytes, terminator only when
    /// the checksum is not `0x0a` itself.
    #[test]
    fn command_frame_pins_the_wire_bytes() {
        // 0x72 + 0x02 + 0x12 + 0x34 = 0xba, so the checksum is 0x46.
        assert_eq!(
            command_frame(LowLevelCommands::ToRead, 0x1234, 0x0002, &[]),
            vec![0x72, 0x02, 0x12, 0x34, 0x46, 0x0A]
        );
        // The payload is escaped and summed escaped: 0x77 + 0x01 + 0xdb +
        // 0xdc = 0x2f, checksum 0xd1.
        assert_eq!(
            command_frame(LowLevelCommands::ToWrite, 0x0000, 0x0001, &[0x0A]),
            vec![0x77, 0x01, 0x00, 0x00, 0xDB, 0xDC, 0xD1, 0x0A]
        );
        assert_eq!(
            command_frame(LowLevelCommands::ToWrite, 0x0000, 0x0001, &[0xDB]),
            vec![0x77, 0x01, 0x00, 0x00, 0xDB, 0xDD, 0xD0, 0x0A]
        );
        // 0x72 + 0x84 = 0xf6, so the checksum is 0x0a, its own terminator.
        assert_eq!(
            command_frame(LowLevelCommands::ToRead, 0x0084, 0x0000, &[]),
            vec![0x72, 0x00, 0x00, 0x84, 0x0A]
        );
    }

    /// `answer_payload` strips the status byte, the checksum and the terminator
    /// before unescaping, and handles a checksum that is itself `0x0a`.
    #[test]
    fn answer_payload_strips_the_framing() {
        // 6f 01 db dc 02 55 0a -> 01 0a 02
        assert_eq!(
            answer_payload(&[0x6F, 0x01, 0xDB, 0xDC, 0x02, 0x55, 0x0A], 6, false),
            vec![0x01, 0x0A, 0x02]
        );
        // An escape pair right against the checksum.
        assert_eq!(
            answer_payload(&[0x6F, 0x11, 0xDB, 0xDC, 0x55, 0x0A], 5, false),
            vec![0x11, 0x0A]
        );
        // Empty and one byte payloads.
        assert_eq!(
            answer_payload(&[0x6F, 0x55, 0x0A], 2, false),
            Vec::<u8>::new()
        );
        assert_eq!(
            answer_payload(&[0x6F, 0x11, 0x55, 0x0A], 3, false),
            vec![0x11]
        );
        // The checksum is itself `0x0a`: no terminator, and it is not payload.
        assert_eq!(
            answer_payload(&[0x6F, 0x11, 0x22, 0x0A], 3, true),
            vec![0x11, 0x22]
        );
        assert_eq!(
            answer_payload(&[0x6F, 0x11, 0xDB, 0xDD, 0x0A], 4, true),
            vec![0x11, 0xDB]
        );
        assert_eq!(answer_payload(&[0x6F, 0x0A], 1, true), Vec::<u8>::new());
        // Degenerate indices cannot slice backwards.
        assert_eq!(answer_payload(&[0x6F], 0, false), Vec::<u8>::new());
        assert_eq!(answer_payload(&[0x6F, 0x0A], 1, false), Vec::<u8>::new());
    }
}
