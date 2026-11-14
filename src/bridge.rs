//! Composing a polled session with the place its statuses go.
//!
//! The bridge is the part that decides *whether* a status is worth publishing;
//! it reaches the broker only through [`StatusSink`] and the inverter only
//! through [`PollingSession`], so both ends can be scripted.

use anyhow::Result;
use thiserror::Error;

use crate::{
    map_protocol::high_level::MapInfo,
    session::{InverterDevice, PollingSession, SessionError},
};

/// Where a polled status goes: the MQTT broker in production, a recorder in
/// tests.
pub trait StatusSink {
    fn publish(&mut self, status: &MapInfo) -> Result<()>;
}

/// Which end of the bridge a failure came from.
///
/// Only that end has to be rebuilt, so a broker that went away does not cost
/// the session its port and its EEPROM read, and a stuck inverter does not cost
/// the broker connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Device,
    Sink,
}

/// A poll that did not make it to the broker, and the end it came from.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error(transparent)]
    Device(#[from] SessionError),
    #[error("cannot publish the MAP status: {0:#}")]
    Sink(anyhow::Error),
}

impl BridgeError {
    pub fn failed_end(&self) -> End {
        match self {
            Self::Device(_) => End::Device,
            Self::Sink(_) => End::Sink,
        }
    }
}

/// Publishes a status only when it differs from the last one that made it out.
///
/// A dashboard keeps the last retained message instead of receiving the same
/// payload again. The comparison is against the last *published* status, not
/// the last polled one, so a status whose publish failed goes out once the sink
/// works again.
pub struct Bridge<S: StatusSink> {
    sink: S,
    last_published: Option<MapInfo>,
}

impl<S: StatusSink> Bridge<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink,
            last_published: None,
        }
    }

    /// Polls the inverter once and publishes the status if it changed.
    ///
    /// A watchdog trip comes out as [`BridgeError::Device`]: a status that
    /// stopped changing needs a fresh session rather than another publish.
    pub fn poll_once<D: InverterDevice>(
        &mut self,
        session: &mut PollingSession<D>,
    ) -> Result<(), BridgeError> {
        let status = session.next_status()?;
        if self.last_published.as_ref() == Some(&status) {
            return Ok(());
        }

        self.sink.publish(&status).map_err(BridgeError::Sink)?;
        self.last_published = Some(status);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        map_protocol::high_level::MapInfo,
        session::{scripted::ScriptedDevice, PollingSession, Watchdog, MAX_CONSECUTIVE_SAME_READS},
    };

    use super::{Bridge, BridgeError, End, StatusSink};

    const INTERVAL: Duration = Duration::from_secs(10);

    fn status(status_char: u8) -> MapInfo {
        MapInfo::for_test(status_char)
    }

    fn session(statuses: Vec<MapInfo>) -> PollingSession<ScriptedDevice> {
        PollingSession::start(ScriptedDevice::map(statuses), Watchdog::new(INTERVAL))
            .expect("a MAP answers")
    }

    /// A sink that keeps every published status, so a test can assert on what
    /// the broker would have seen.
    #[derive(Debug, Default)]
    struct RecordingSink {
        published: Vec<MapInfo>,
    }

    impl StatusSink for RecordingSink {
        fn publish(&mut self, status: &MapInfo) -> anyhow::Result<()> {
            self.published.push(status.clone());
            Ok(())
        }
    }

    /// A sink that cannot reach its broker.
    #[derive(Debug)]
    struct FailingSink;

    impl StatusSink for FailingSink {
        fn publish(&mut self, _status: &MapInfo) -> anyhow::Result<()> {
            anyhow::bail!("the broker is away")
        }
    }

    #[test]
    fn only_changes_are_published() {
        let statuses = [
            status(1),
            status(1),
            status(2),
            status(2),
            status(2),
            status(1),
        ];
        let mut session = session(statuses.to_vec());
        let mut bridge = Bridge::new(RecordingSink::default());

        for _ in 0..statuses.len() {
            bridge
                .poll_once(&mut session)
                .expect("the script has a status");
        }

        assert_eq!(
            bridge.sink.published,
            vec![status(1), status(2), status(1)],
            "each change is published once and a repeat is not"
        );
    }

    #[test]
    fn an_unchanged_status_is_never_published() {
        let mut session = session(vec![status(1); MAX_CONSECUTIVE_SAME_READS as usize + 1]);
        let mut bridge = Bridge::new(RecordingSink::default());

        for _ in 0..=MAX_CONSECUTIVE_SAME_READS {
            bridge
                .poll_once(&mut session)
                .expect("the script has a status");
        }

        assert_eq!(bridge.sink.published, vec![status(1)]);
    }

    #[test]
    fn a_stuck_inverter_ends_the_polling_with_a_device_error() {
        let mut session = session(vec![status(1); MAX_CONSECUTIVE_SAME_READS as usize + 2]);
        let mut bridge = Bridge::new(RecordingSink::default());

        for _ in 0..=MAX_CONSECUTIVE_SAME_READS {
            bridge
                .poll_once(&mut session)
                .expect("a status the watchdog allows");
        }

        let error = bridge
            .poll_once(&mut session)
            .expect_err("one more identical read trips the watchdog");
        assert!(matches!(error, BridgeError::Device(_)));
        assert_eq!(error.failed_end(), End::Device);
    }

    #[test]
    fn a_publish_that_fails_is_blamed_on_the_sink() {
        let mut session = session(vec![status(1)]);
        let mut bridge = Bridge::new(FailingSink);

        let error = bridge
            .poll_once(&mut session)
            .expect_err("the broker is away");

        assert!(matches!(error, BridgeError::Sink(_)));
        assert_eq!(
            error.failed_end(),
            End::Sink,
            "a broker that went away must not cost the session its port"
        );
    }

    /// The sink is replaced, the session is not: the status the failed publish
    /// lost goes out on the next poll, even though the inverter did not change.
    #[test]
    fn a_status_lost_to_a_publish_failure_goes_out_after_the_reconnect() {
        let mut session = session(vec![status(1), status(1)]);
        let mut bridge = Bridge::new(FailingSink);
        bridge
            .poll_once(&mut session)
            .expect_err("the broker is away");

        let mut bridge = Bridge::new(RecordingSink::default());
        bridge.poll_once(&mut session).expect("the broker is back");

        assert_eq!(bridge.sink.published, vec![status(1)]);
    }
}
