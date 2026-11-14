//! Reads a MAP inverter and publishes its status to MQTT.
//!
//! `main` only composes: [`SerialDevice`] and [`MqttSink`] in production, one
//! retry policy around them. The polling lives in [`session`], the
//! publish-on-change rule and the sink seam in [`bridge`].

use std::{thread, time::Duration};

use log::warn;

use bridge::{Bridge, BridgeError, End};
use cli::{MqttParams, SerialParams, WorkingMode};
use map_protocol::high_level::ensure_map_present;
use mqtt_sink::MqttSink;
use session::{InverterDevice, PollingSession, SerialDevice, Watchdog};

mod bridge;
mod cli;
mod map_protocol;
mod mqtt_sink;
mod session;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    match cli::parse().mode {
        WorkingMode::Completion { shell } => {
            let mut command = cli::command();
            cli::print_completions(shell, &mut command);
        }
        WorkingMode::Stdout {
            serial_params,
            json_output,
        } => dump_status_once(&serial_params, json_output)?,
        WorkingMode::Mqtt(params) => run_mqtt(&params)?,
    }

    Ok(())
}

/// Reads the status once and prints it, without an MQTT broker in the way.
fn dump_status_once(serial_params: &SerialParams, json_output: bool) -> anyhow::Result<()> {
    let mut device = SerialDevice::open(&serial_params.map_port, serial_params.map_port_speed)?;
    let eeprom = device.read_eeprom()?;
    ensure_map_present(&eeprom)?;
    let status = device.read_status(&eeprom)?;

    if json_output {
        print!("{}", serde_json::to_string_pretty(&status)?);
    } else {
        log::debug!("{status:?}");
    }

    Ok(())
}

/// Polls the inverter and publishes it until the process is stopped.
///
/// This is the one retry policy of the bridge: whatever failed is rebuilt after
/// `--reconnect-every`, and nothing else is.
fn run_mqtt(params: &MqttParams) -> anyhow::Result<()> {
    let interval = Duration::from(&params.interval);
    let reconnect_every = params.reconnect_every.as_ref().map(Duration::from);
    let mut links = Links::new(params, interval);

    loop {
        match links.poll_once() {
            Ok(()) => thread::sleep(interval),
            Err(error) => {
                let Some(reconnect_every) = reconnect_every else {
                    return Err(error.into());
                };
                warn!("{error:?}");
                thread::sleep(reconnect_every);
                links.discard(error.failed_end());
            }
        }
    }
}

/// The two ends of the bridge, each kept until it fails.
struct Links<'a> {
    params: &'a MqttParams,
    interval: Duration,
    session: Option<PollingSession<SerialDevice>>,
    bridge: Option<Bridge<MqttSink>>,
}

impl<'a> Links<'a> {
    fn new(params: &'a MqttParams, interval: Duration) -> Self {
        Self {
            params,
            interval,
            session: None,
            bridge: None,
        }
    }

    /// Opens whichever end is missing and polls once.
    fn poll_once(&mut self) -> Result<(), BridgeError> {
        if self.session.is_none() {
            let device = SerialDevice::open(
                &self.params.serial_params.map_port,
                self.params.serial_params.map_port_speed,
            )?;
            self.session = Some(PollingSession::start(device, Watchdog::new(self.interval))?);
        }
        if self.bridge.is_none() {
            let sink = MqttSink::connect(self.params).map_err(BridgeError::Sink)?;
            self.bridge = Some(Bridge::new(sink));
        }

        let session = self.session.as_mut().expect("just opened");
        let bridge = self.bridge.as_mut().expect("just connected");
        bridge.poll_once(session)
    }

    /// Forgets the end that failed, so the next poll rebuilds it.
    fn discard(&mut self, end: End) {
        match end {
            End::Device => self.session = None,
            End::Sink => self.bridge = None,
        }
    }
}
