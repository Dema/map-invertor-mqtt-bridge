//! Reads a MAP inverter and publishes its status to MQTT.
//!
//! `main` only composes: [`SerialDevice`] and [`MqttSink`] in production, one
//! retry policy around them. The polling lives in [`session`], the
//! publish-on-change rule and the sink seam in [`bridge`].

use std::{thread, time::Duration};

use log::warn;

use bridge::Links;
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
    let mut links = Links::new();

    loop {
        match links.poll_once(
            || {
                let device = SerialDevice::open(
                    &params.serial_params.map_port,
                    params.serial_params.map_port_speed,
                )?;
                PollingSession::start(device, Watchdog::new(interval))
            },
            || MqttSink::connect(params),
        ) {
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
