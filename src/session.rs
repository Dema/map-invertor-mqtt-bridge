//! The polling side of the bridge: one MAP session, polled until it fails.
//!
//! A session owns everything that belongs to the device rather than to the
//! broker: the open port, the EEPROM read from it, and the watchdog that
//! notices a status which stopped changing. It knows nothing about MQTT, so a
//! scripted device can stand in for the inverter.

use std::time::Duration;

use log::info;
use thiserror::Error;

use crate::map_protocol::{
    high_level::{ensure_map_present, HighLevelProtocol, MapInfo},
    MapError,
};

/// Consecutive identical status reads after which the session is considered
/// stuck. One more identical read than this trips the watchdog.
pub const MAX_CONSECUTIVE_SAME_READS: u32 = 10;

/// How long a single read waits for the inverter to answer.
const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// The device a session polls: a MAP on a serial port in production, a script
/// in tests.
pub trait InverterDevice {
    fn read_eeprom(&mut self) -> Result<[u8; 560], MapError>;
    fn read_status(&mut self, eeprom: &[u8; 560]) -> Result<MapInfo, MapError>;
}

/// The production device: the MAP protocol over an open serial port.
#[derive(Debug)]
pub struct SerialDevice {
    protocol: HighLevelProtocol,
}

impl SerialDevice {
    pub fn open(port_name: &str, speed: u32) -> Result<Self, SessionError> {
        let port = serialport::new(port_name, speed)
            .timeout(READ_TIMEOUT)
            .open()
            .map_err(|source| SessionError::Open {
                port: port_name.to_string(),
                source,
            })?;
        info!("Map port {} opened", port_name);
        Ok(Self {
            protocol: HighLevelProtocol::new(port)?,
        })
    }
}

impl InverterDevice for SerialDevice {
    fn read_eeprom(&mut self) -> Result<[u8; 560], MapError> {
        self.protocol.read_eeprom()
    }

    fn read_status(&mut self, eeprom: &[u8; 560]) -> Result<MapInfo, MapError> {
        self.protocol.read_status(eeprom)
    }
}

/// The status has not changed for too long, so the session has to be rebuilt.
#[derive(Debug, PartialEq, Eq, Error)]
#[error("map info not changed for {seconds} seconds and {reads} iterations, restarting")]
pub struct WatchdogTrip {
    /// How long the status has been the same, at the configured interval.
    pub seconds: u64,
    /// How many times in a row the same status was read.
    pub reads: u32,
}

/// Trips when the inverter's status stops changing.
///
/// A status that never changes is how a MAP that hung or lost its firmware
/// looks from here, and the only cure is a fresh session, so the watchdog turns
/// the staleness into an error the caller already handles.
#[derive(Debug)]
pub struct Watchdog {
    interval: Duration,
    max_same_reads: u32,
    previous: Option<MapInfo>,
    same_reads: u32,
}

impl Watchdog {
    /// A watchdog for a session polled every `interval`.
    pub fn new(interval: Duration) -> Self {
        Self::with_max_same_reads(interval, MAX_CONSECUTIVE_SAME_READS)
    }

    pub fn with_max_same_reads(interval: Duration, max_same_reads: u32) -> Self {
        Self {
            interval,
            max_same_reads,
            previous: None,
            same_reads: 0,
        }
    }

    /// Records one read and reports whether it differs from the read before it.
    ///
    /// The first read always counts as a change. Once the status has been the
    /// same for more than `max_same_reads` reads the watchdog trips instead of
    /// answering.
    pub fn observe(&mut self, status: &MapInfo) -> Result<bool, WatchdogTrip> {
        if self.previous.as_ref() != Some(status) {
            self.previous = Some(status.clone());
            self.same_reads = 0;
            return Ok(true);
        }

        self.same_reads += 1;
        if self.same_reads > self.max_same_reads {
            return Err(WatchdogTrip {
                seconds: self.interval.as_secs() * u64::from(self.same_reads),
                reads: self.same_reads,
            });
        }
        Ok(false)
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error(transparent)]
    Map(#[from] MapError),
    #[error("cannot open the MAP port {port}: {source}")]
    Open {
        port: String,
        source: serialport::Error,
    },
    #[error(transparent)]
    Watchdog(#[from] WatchdogTrip),
}

/// One MAP session: the EEPROM read once, then the status polled from the same
/// port for as long as the session lasts.
#[derive(Debug)]
pub struct PollingSession<D: InverterDevice> {
    device: D,
    eeprom: [u8; 560],
    watchdog: Watchdog,
}

impl<D: InverterDevice> PollingSession<D> {
    /// Opens the session on `device`: reads its EEPROM once and refuses to poll
    /// a device that is not a MAP.
    ///
    /// Everything the EEPROM config contributes to a status is decoded from the
    /// copy kept here, so it is read once per session rather than once per poll.
    pub fn start(mut device: D, watchdog: Watchdog) -> Result<Self, SessionError> {
        let eeprom = device.read_eeprom()?;
        ensure_map_present(&eeprom)?;
        Ok(Self {
            device,
            eeprom,
            watchdog,
        })
    }

    /// Polls the status once.
    ///
    /// The watchdog sees every read, so a status that stops changing ends the
    /// session with an error instead of being polled forever.
    pub fn next_status(&mut self) -> Result<MapInfo, SessionError> {
        let status = self.device.read_status(&self.eeprom)?;
        self.watchdog.observe(&status)?;
        Ok(status)
    }
}

#[cfg(test)]
pub(crate) mod scripted {
    use std::collections::VecDeque;

    use crate::map_protocol::{
        high_level::{MapInfo, MAP_PRESENT_MAGIC},
        MapError,
    };

    use super::InverterDevice;

    /// A device that hands out scripted statuses in order and remembers how
    /// often it was asked for the EEPROM.
    #[derive(Debug)]
    pub(crate) struct ScriptedDevice {
        eeprom: [u8; 560],
        statuses: VecDeque<MapInfo>,
        pub(crate) eeprom_reads: usize,
        pub(crate) status_reads: usize,
    }

    impl ScriptedDevice {
        /// A MAP that answers with `statuses`, in that order.
        pub(crate) fn map(statuses: Vec<MapInfo>) -> Self {
            let mut eeprom = [0u8; 560];
            eeprom[0] = MAP_PRESENT_MAGIC;
            Self {
                eeprom,
                statuses: statuses.into(),
                eeprom_reads: 0,
                status_reads: 0,
            }
        }

        /// A device that answers, but is not a MAP.
        pub(crate) fn not_a_map() -> Self {
            Self {
                eeprom: [0u8; 560],
                statuses: VecDeque::new(),
                eeprom_reads: 0,
                status_reads: 0,
            }
        }
    }

    impl InverterDevice for ScriptedDevice {
        fn read_eeprom(&mut self) -> Result<[u8; 560], MapError> {
            self.eeprom_reads += 1;
            Ok(self.eeprom)
        }

        fn read_status(&mut self, _eeprom: &[u8; 560]) -> Result<MapInfo, MapError> {
            self.status_reads += 1;
            Ok(self
                .statuses
                .pop_front()
                .expect("the script ran out of statuses"))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::map_protocol::{high_level::MapInfo, MapError};

    use super::{
        scripted::ScriptedDevice, PollingSession, SessionError, Watchdog, WatchdogTrip,
        MAX_CONSECUTIVE_SAME_READS,
    };

    const INTERVAL: Duration = Duration::from_secs(10);

    /// A status that differs from `MapInfo::default()` only in `status_char`.
    fn status(status_char: u8) -> MapInfo {
        MapInfo::for_test(status_char)
    }

    #[test]
    fn the_first_read_counts_as_a_change() {
        let mut watchdog = Watchdog::new(INTERVAL);
        assert_eq!(watchdog.observe(&status(1)), Ok(true));
    }

    #[test]
    fn an_unchanged_status_is_not_a_change() {
        let mut watchdog = Watchdog::new(INTERVAL);
        assert_eq!(watchdog.observe(&status(1)), Ok(true));
        for _ in 0..MAX_CONSECUTIVE_SAME_READS {
            assert_eq!(watchdog.observe(&status(1)), Ok(false));
        }
    }

    #[test]
    fn the_watchdog_trips_after_one_read_more_than_the_limit() {
        let mut watchdog = Watchdog::new(INTERVAL);
        watchdog.observe(&status(1)).expect("the first read");

        for _ in 0..MAX_CONSECUTIVE_SAME_READS {
            assert_eq!(watchdog.observe(&status(1)), Ok(false));
        }

        let reads = MAX_CONSECUTIVE_SAME_READS + 1;
        assert_eq!(
            watchdog.observe(&status(1)),
            Err(WatchdogTrip {
                seconds: INTERVAL.as_secs() * u64::from(reads),
                reads,
            })
        );
    }

    #[test]
    fn a_change_resets_the_watchdog() {
        let mut watchdog = Watchdog::with_max_same_reads(INTERVAL, 2);
        watchdog.observe(&status(1)).expect("the first read");
        watchdog.observe(&status(1)).expect("the first repeat");
        watchdog.observe(&status(1)).expect("the second repeat");

        assert_eq!(watchdog.observe(&status(2)), Ok(true));
        // The counter starts over, so the new status gets its own budget.
        assert_eq!(watchdog.observe(&status(2)), Ok(false));
        assert_eq!(watchdog.observe(&status(2)), Ok(false));
        assert!(watchdog.observe(&status(2)).is_err());
    }

    #[test]
    fn a_session_reads_the_eeprom_once_and_polls_statuses_without_it() {
        let mut session = PollingSession::start(
            ScriptedDevice::map(vec![status(1), status(2), status(2)]),
            Watchdog::new(INTERVAL),
        )
        .expect("a MAP answers");

        assert_eq!(session.next_status().expect("first poll"), status(1));
        assert_eq!(session.next_status().expect("second poll"), status(2));
        assert_eq!(session.next_status().expect("third poll"), status(2));

        assert_eq!(session.device.eeprom_reads, 1);
        assert_eq!(session.device.status_reads, 3);
    }

    #[test]
    fn a_session_refuses_a_device_that_is_not_a_map() {
        let result = PollingSession::start(ScriptedDevice::not_a_map(), Watchdog::new(INTERVAL));

        assert!(matches!(result, Err(SessionError::Map(MapError::NotFound))));
    }

    #[test]
    fn a_port_that_cannot_be_opened_is_a_session_error() {
        let result = super::SerialDevice::open("/dev/map-that-is-not-there", 9600);

        assert!(matches!(result, Err(SessionError::Open { .. })));
    }

    #[test]
    fn a_session_reports_a_tripped_watchdog_as_a_session_error() {
        let mut session = PollingSession::start(
            ScriptedDevice::map(vec![status(1); MAX_CONSECUTIVE_SAME_READS as usize + 2]),
            Watchdog::new(INTERVAL),
        )
        .expect("a MAP answers");

        for _ in 0..=MAX_CONSECUTIVE_SAME_READS {
            session
                .next_status()
                .expect("a status the watchdog still allows");
        }

        assert!(matches!(
            session.next_status(),
            Err(SessionError::Watchdog(WatchdogTrip { .. }))
        ));
    }
}
