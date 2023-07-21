use std::{fmt::Debug, thread, time::Duration};

use clap::{Args, Command, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use map_protocol::high_level::HighLevelProtocol;
use rumqttc::{Client, MqttOptions, Packet, Publish};

use crate::map_protocol::MapError;

mod map_protocol;

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
        #[arg(short, long, env)]
        map_port: String,
        /// MQTT broker hostname
        #[arg(short, long, env)]
        mqtt_hostname: String,
        /// MQTT broker port
        #[arg(short, long, env)]
        mqtt_port: u16,
        /// MQTT broker topic
        #[arg(short, long, env)]
        mqtt_topic: String,
    },
    Stdout {
        /// Map port
        #[arg(short, long, env)]
        map_port: String,
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
            eprintln!("{}", error);
            std::process::exit(1);
        }
    };
}

fn main_with_error(args: Cli) -> Result<(), anyhow::Error> {
    match args.mode {
        WorkingMode::Completion { shell } => {
            let cli = Command::new("CLI");
            let mut cli = Cli::augment_args(cli);
            print_completions(shell, &mut cli)
        }
        WorkingMode::Stdout {
            map_port,
            json_output,
        } => {
            let mut protocol = HighLevelProtocol::new(map_port)?;

            let eeprom = protocol.read_eeprom()?;

            if eeprom[0] != 3 {
                return Err(MapError::NotFound.into());
            }
            let map_info = protocol.read_status()?;
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
            // map_port,
            mqtt_hostname,
            mqtt_port,
            mqtt_topic,
        } => {
            let mut protocol = HighLevelProtocol::new(map_port)?;
            let mut mqttoptions = MqttOptions::new("rumqtt-sync", mqtt_hostname, mqtt_port);
            mqttoptions.set_keep_alive(Duration::from_secs(5));

            let (mut client, mut connection) = Client::new(mqttoptions, 10);
            let eeprom = protocol.read_eeprom()?;

            if eeprom[0] != 3 {
                return Err(MapError::NotFound.into());
            }
            loop {
                let map_info = protocol.read_status()?;

                client.publish(
                    &mqtt_topic,
                    rumqttc::QoS::AtLeastOnce,
                    false,
                    serde_json::to_vec(&map_info)?,
                )?;
                thread::sleep(Duration::from_secs(10));
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
