use std::{fmt::Debug, thread, time::Duration};

use crate::map_protocol::MapError;
use clap::{Args, Command, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_duration::duration_range_value_parse;

use duration_human::{DurationHuman, DurationHumanValidator};
use map_protocol::{high_level::HighLevelProtocol, NotFoundSnafu};
use rumqttc::{Client, ClientError, MqttOptions};

use snafu::{Backtrace, ErrorCompat, ResultExt, Snafu};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

mod map_protocol;

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
enum MainError {
    Map {
        #[snafu(backtrace)]
        source: MapError,
    },
    #[snafu(display("SerialPort Error {source}"))]
    SerialPort {
        source: serialport::Error,
        backtrace: Backtrace,
    },
    #[snafu(display("MQTT error"))]
    Mqtt {
        source: ClientError,
        backtrace: Backtrace,
    },
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    mode: WorkingMode,
}

#[derive(Clone, Debug, Subcommand)]
enum WorkingMode {
    Mqtt {
        /// Map port
        #[arg(short = 'p', long, env)]
        map_port: String,
        /// Map port peed
        #[arg(short = 's', long, env)]
        map_port_speed: u32,
        /// MQTT broker hostname
        #[arg(short, long, env)]
        mqtt_hostname: String,
        /// MQTT broker port
        #[arg(long, env)]
        mqtt_port: u16,
        /// MQTT broker topic
        #[arg(long, env)]
        mqtt_topic: String,
        /// Polling interval
        #[arg(
        long, default_value="10s",
        value_parser = duration_range_value_parse!(min: 1s, max: 10min)
    )]
        interval: DurationHuman,
    },
    Stdout {
        /// Map port
        #[arg(short = 'p', long, env)]
        map_port: String,
        /// Map port speed
        #[arg(short = 's', long, env)]
        map_port_speed: u32,
        /// When not using MQTT dump to stdout as JSON instead of human readable text
        #[arg(short, long)]
        json_output: bool,
    },
    Completion {
        /// generate autcompletion script for shell
        #[arg(short, long)]
        shell: Shell,
    },
}

fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}
fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Command::new("CLI");
    let cli = Cli::augment_args(cli);
    let matches = cli.get_matches();
    let args = Cli::from_arg_matches(&matches);
    let args = match args {
        Ok(args) => args,
        Err(error) => {
            eprint!("{}", error.render());
            std::process::exit(1);
        }
    };

    match main_with_error(args) {
        Ok(_) => {}
        Err(error) => {
            eprintln!("{}", &error);
            if let Some(b) = ErrorCompat::backtrace(&error) {
                eprintln!("{}", b);
            }
            std::process::exit(1);
        }
    };
}

fn main_with_error(args: Cli) -> Result<(), MainError> {
    match args.mode {
        WorkingMode::Completion { shell } => {
            let cli = Command::new("CLI");
            let mut cli = Cli::augment_args(cli);
            print_completions(shell, &mut cli)
        }
        WorkingMode::Stdout {
            map_port,
            map_port_speed,
            json_output,
        } => {
            let port = serialport::new(map_port, map_port_speed)
                .timeout(Duration::from_secs(20))
                .open()
                .context(SerialPortSnafu {})?;
            let mut protocol = HighLevelProtocol::new(port).context(MapSnafu {})?;
            // protocol.temp_read_status();
            let eeprom = protocol.read_eeprom().context(MapSnafu {})?;
            if eeprom[0] != 3 {
                return NotFoundSnafu {}.fail().context(MapSnafu {})?;
            }
            let map_info = protocol.read_status(&eeprom).context(MapSnafu {})?;
            if json_output {
                print!(
                    "{}",
                    serde_json::to_string_pretty(&map_info)
                        .expect("Cannot serialize map_info value")
                );
            } else {
                dbg!(map_info);
            };
        }
        WorkingMode::Mqtt {
            map_port,
            map_port_speed,
            mqtt_hostname,
            mqtt_port,
            mqtt_topic,
            interval,
        } => {
            let port = serialport::new(map_port, map_port_speed)
                .timeout(Duration::from_secs(20))
                .open()
                .context(SerialPortSnafu {})?;
            let mut protocol = HighLevelProtocol::new(port).context(MapSnafu {})?;
            let mut mqttoptions = MqttOptions::new("rumqtt-sync", mqtt_hostname, mqtt_port);
            mqttoptions.set_keep_alive(Duration::from_secs(5));

            let (mut client, _) = Client::new(mqttoptions, 10);
            let eeprom = protocol.read_eeprom().context(MapSnafu {})?;

            if eeprom[0] != 3 {
                return NotFoundSnafu {}.fail().context(MapSnafu {})?;
            }
            loop {
                let map_info = protocol.read_status(&eeprom).context(MapSnafu {})?;

                client
                    .publish(
                        &mqtt_topic,
                        rumqttc::QoS::AtLeastOnce,
                        false,
                        serde_json::to_vec(&map_info).unwrap(),
                    )
                    .context(MqttSnafu {})?;
                thread::sleep(Duration::from(&interval));
            }
        }
    }
    Ok(())
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
#[test]
fn temp_copy() {
    let src = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let mut dst = [0u8; 5];
    dst[0..3].clone_from_slice(&src[0..3]);
    dbg!(dst);
}
