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

    /// Whether the sink can reach its destination right now.
    ///
    /// A sink that cannot is asked to publish anyway: the failure is what tells
    /// the caller to rebuild it, and it is also what gets the last status back
    /// to a broker that restarted and lost its retained message.
    fn is_connected(&mut self) -> bool {
        true
    }
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
/// works again, and so does a status whose broker restarted underneath it.
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
        if self.sink.is_connected() && self.last_published.as_ref() == Some(&status) {
            return Ok(());
        }

        self.sink.publish(&status).map_err(BridgeError::Sink)?;
        self.last_published = Some(status);
        Ok(())
    }
}

/// The two ends of the bridge, each kept until it fails.
///
/// A failure clears only the end it came from, so a session keeps its port and
/// its EEPROM read across broker outages and the broker connection keeps its
/// socket across inverter outages. Both ends are built on demand by the
/// factories `poll_once` is given, which is what lets a test count how often a
/// session was really rebuilt.
pub struct Links<D: InverterDevice, S: StatusSink> {
    session: Option<PollingSession<D>>,
    bridge: Option<Bridge<S>>,
}

impl<D: InverterDevice, S: StatusSink> Default for Links<D, S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: InverterDevice, S: StatusSink> Links<D, S> {
    pub fn new() -> Self {
        Self {
            session: None,
            bridge: None,
        }
    }

    /// Opens whichever end is missing and polls once.
    ///
    /// The factories are called only for an end that is not open already, and
    /// their errors are blamed on the end they were building.
    pub fn poll_once<OpenDevice, ConnectSink>(
        &mut self,
        open_device: OpenDevice,
        connect_sink: ConnectSink,
    ) -> Result<(), BridgeError>
    where
        OpenDevice: FnOnce() -> Result<PollingSession<D>, SessionError>,
        ConnectSink: FnOnce() -> Result<S>,
    {
        if self.session.is_none() {
            self.session = Some(open_device()?);
        }
        if self.bridge.is_none() {
            self.bridge = Some(Bridge::new(connect_sink().map_err(BridgeError::Sink)?));
        }

        let session = self.session.as_mut().expect("just opened");
        let bridge = self.bridge.as_mut().expect("just opened");
        bridge.poll_once(session)
    }

    /// Forgets the end that failed, so the next poll rebuilds it.
    pub fn discard(&mut self, end: End) {
        match end {
            End::Device => self.session = None,
            End::Sink => self.bridge = None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        map_protocol::{high_level::MapInfo, MapError},
        session::{
            scripted::ScriptedDevice, PollingSession, SessionError, Watchdog,
            MAX_CONSECUTIVE_SAME_READS,
        },
    };

    use super::{Bridge, BridgeError, End, Links, StatusSink};

    const INTERVAL: Duration = Duration::from_secs(10);

    fn status(status_char: u8) -> MapInfo {
        MapInfo::for_test(status_char)
    }

    fn session(statuses: Vec<MapInfo>) -> PollingSession<ScriptedDevice> {
        PollingSession::start(ScriptedDevice::map(statuses), Watchdog::new(INTERVAL))
            .expect("a MAP answers")
    }

    fn watchdog() -> Watchdog {
        Watchdog::new(INTERVAL)
    }

    /// A sink that keeps every published status, so a test can assert on what
    /// the broker would have seen, and that can pretend the broker went away.
    #[derive(Debug)]
    struct ScriptedSink {
        published: Vec<MapInfo>,
        connected: bool,
    }

    impl Default for ScriptedSink {
        fn default() -> Self {
            Self {
                published: Vec::new(),
                connected: true,
            }
        }
    }

    impl StatusSink for ScriptedSink {
        fn publish(&mut self, status: &MapInfo) -> anyhow::Result<()> {
            if !self.connected {
                anyhow::bail!("the broker is away");
            }
            self.published.push(status.clone());
            Ok(())
        }

        fn is_connected(&mut self) -> bool {
            self.connected
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
        let mut bridge = Bridge::new(ScriptedSink::default());

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
        let mut bridge = Bridge::new(ScriptedSink::default());

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
        let mut bridge = Bridge::new(ScriptedSink::default());

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
        let mut bridge = Bridge::new(ScriptedSink {
            connected: false,
            ..Default::default()
        });

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
        let mut bridge = Bridge::new(ScriptedSink {
            connected: false,
            ..Default::default()
        });
        bridge
            .poll_once(&mut session)
            .expect_err("the broker is away");

        let mut bridge = Bridge::new(ScriptedSink::default());
        bridge.poll_once(&mut session).expect("the broker is back");

        assert_eq!(bridge.sink.published, vec![status(1)]);
    }

    /// A broker that restarted lost its retained message, so the status has to
    /// go out again even though it did not change.
    #[test]
    fn a_status_is_republished_when_the_broker_went_away() {
        let mut session = session(vec![status(1); 3]);
        let mut bridge = Bridge::new(ScriptedSink::default());
        bridge.poll_once(&mut session).expect("the first status");

        bridge.sink.connected = false;
        let error = bridge
            .poll_once(&mut session)
            .expect_err("the broker must be noticed before the status changes");
        assert_eq!(error.failed_end(), End::Sink);

        let mut bridge = Bridge::new(ScriptedSink::default());
        bridge.poll_once(&mut session).expect("the broker is back");
        assert_eq!(bridge.sink.published, vec![status(1)]);
    }

    /// An unresponsive inverter costs a new session and nothing else, so the
    /// EEPROM is read again only because the port was reopened.
    #[test]
    fn a_device_failure_rebuilds_only_the_session() {
        let mut links: Links<ScriptedDevice, ScriptedSink> = Links::new();
        let mut device_opens = 0;
        let mut sink_opens = 0;

        let unresponsive =
            ScriptedDevice::scripted(vec![Err(MapError::UnknownValueError { value: 0 })]);
        let error = links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(unresponsive, watchdog())
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink::default())
                },
            )
            .expect_err("the inverter does not answer");
        assert_eq!(error.failed_end(), End::Device);
        links.discard(error.failed_end());

        links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(ScriptedDevice::map(vec![status(1)]), watchdog())
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink::default())
                },
            )
            .expect("the inverter answers again");

        assert_eq!(device_opens, 2, "the failed session is rebuilt");
        assert_eq!(sink_opens, 1, "the broker connection is kept");
    }

    /// A broker outage costs a broker connection and nothing else: the session,
    /// and with it the port and the EEPROM read, is kept.
    #[test]
    fn a_broker_failure_rebuilds_only_the_sink() {
        let mut links: Links<ScriptedDevice, ScriptedSink> = Links::new();
        let mut device_opens = 0;
        let mut sink_opens = 0;

        let error = links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(
                        ScriptedDevice::map(vec![status(1), status(1)]),
                        watchdog(),
                    )
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink {
                        connected: false,
                        ..Default::default()
                    })
                },
            )
            .expect_err("the broker is away");
        assert_eq!(error.failed_end(), End::Sink);
        links.discard(error.failed_end());

        links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(ScriptedDevice::map(Vec::new()), watchdog())
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink::default())
                },
            )
            .expect("the broker is back");

        assert_eq!(
            device_opens, 1,
            "the session keeps its port and its EEPROM read"
        );
        assert_eq!(sink_opens, 2, "the broker connection is rebuilt");
    }

    /// A reconnect that keeps failing must not cost the healthy end either.
    #[test]
    fn a_broker_connection_that_cannot_be_reopened_keeps_the_session() {
        let mut links: Links<ScriptedDevice, ScriptedSink> = Links::new();
        let mut device_opens = 0;
        let mut sink_opens = 0;

        let error = links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(ScriptedDevice::map(vec![status(1); 3]), watchdog())
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink {
                        connected: false,
                        ..Default::default()
                    })
                },
            )
            .expect_err("the broker is away");
        assert_eq!(error.failed_end(), End::Sink);
        links.discard(error.failed_end());

        for _ in 0..2 {
            let error = links
                .poll_once(
                    || panic!("the session is still healthy"),
                    || {
                        sink_opens += 1;
                        anyhow::bail!("still away")
                    },
                )
                .expect_err("the broker is still away");
            assert_eq!(error.failed_end(), End::Sink);
            links.discard(error.failed_end());
        }

        assert_eq!(device_opens, 1, "the session was never rebuilt");
        assert_eq!(sink_opens, 3, "every attempt rebuilt the broker connection");

        links
            .poll_once(
                || panic!("the session is still healthy"),
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink::default())
                },
            )
            .expect("the broker is back");
        assert_eq!(device_opens, 1);
    }

    /// The same for a port that will not reopen: the broker connection stays.
    #[test]
    fn a_session_that_cannot_be_reopened_keeps_the_broker_connection() {
        let mut links: Links<ScriptedDevice, ScriptedSink> = Links::new();
        let mut device_opens = 0;
        let mut sink_opens = 0;

        links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(
                        ScriptedDevice::scripted(vec![
                            Ok(status(1)),
                            Err(MapError::UnknownValueError { value: 0 }),
                        ]),
                        watchdog(),
                    )
                },
                || {
                    sink_opens += 1;
                    Ok(ScriptedSink::default())
                },
            )
            .expect("the first status");

        let error = links
            .poll_once(
                || panic!("the session is open"),
                || panic!("the sink is open"),
            )
            .expect_err("the inverter stops answering");
        assert_eq!(error.failed_end(), End::Device);
        links.discard(error.failed_end());

        for _ in 0..2 {
            let error = links
                .poll_once(
                    || {
                        device_opens += 1;
                        Err(SessionError::Open {
                            port: "/dev/map".to_string(),
                            source: serialport::Error::new(serialport::ErrorKind::NoDevice, "gone"),
                        })
                    },
                    || panic!("the sink is open"),
                )
                .expect_err("the port is still gone");
            assert_eq!(error.failed_end(), End::Device);
            links.discard(error.failed_end());
        }

        assert_eq!(device_opens, 3, "every attempt reopened the port");
        assert_eq!(sink_opens, 1, "the broker connection was never rebuilt");

        links
            .poll_once(
                || {
                    device_opens += 1;
                    PollingSession::start(ScriptedDevice::map(vec![status(1)]), watchdog())
                },
                || panic!("the sink is open"),
            )
            .expect("the inverter answers again");
        assert_eq!(sink_opens, 1);
    }
}
