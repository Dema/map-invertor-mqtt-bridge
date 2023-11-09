use log::{self, info, trace, warn};
use std::{
    fmt::{format, Debug},
    thread,
    time::Duration,
};

use anyhow::bail;
use clap::{Args, Command, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_duration::duration_range_value_parse;
use duration_human::{DurationHuman, DurationHumanValidator};
use map_protocol::high_level::{HighLevelProtocol, MapInfo};

use paho_mqtt::{Message, QOS_1};

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
        /// MQTT broker username
        #[arg(long, env)]
        mqtt_username: String,
        /// MQTT broker password
        #[arg(long, env)]
        mqtt_password: String,
        /// MQTT broker topic
        #[arg(long, env)]
        mqtt_topic: Option<String>,
        /// my id, default is "map-invertor-mqtt-bridge"
        #[arg(long, env)]
        mqtt_id: Option<String>,
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
fn main() -> anyhow::Result<()> {
    env_logger::init();

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

    match args.mode {
        WorkingMode::Completion { shell } => {
            let cli = Command::new("CLI");
            let mut cli = Cli::augment_args(cli);
            print_completions(shell, &mut cli);
        }
        WorkingMode::Stdout {
            map_port,
            map_port_speed,
            json_output,
        } => {
            let port = serialport::new(map_port, map_port_speed)
                .timeout(Duration::from_secs(20))
                .open()?;
            let mut protocol = HighLevelProtocol::new(port)?;

            let eeprom = protocol.read_eeprom()?;
            if eeprom[0] != 3 {
                bail!("MAP not found");
            }
            let map_info = protocol.read_status(&eeprom)?;
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
            mqtt_username,
            mqtt_password,
            mqtt_topic,
            mqtt_id,
            interval,
        } => {
            let port = serialport::new(&map_port, map_port_speed)
                .timeout(Duration::from_secs(20))
                .open()?;
            info!("Map port {} opened", map_port);
            let mut map_protocol = HighLevelProtocol::new(port)?;
            let mqtt_id = mqtt_id.unwrap_or("map-invertor-mqtt-bridge".into());

            let url: String = format!("tcp://{mqtt_hostname}:{mqtt_port}");

            let cli = paho_mqtt::Client::new((url.clone(), mqtt_id))?;
            let conn_opts = paho_mqtt::ConnectOptionsBuilder::new()
                .keep_alive_interval(Duration::from_secs(20))
                .user_name(&mqtt_username)
                .password(&mqtt_password)
                .clean_session(true)
                .finalize();

            cli.connect(conn_opts)?;
            info!("connected to MQTT broker at {}", url);
            let eeprom = map_protocol.read_eeprom()?;

            if eeprom[0] != 3 {
                bail!("MAP not found");
            }
            let topic = &mqtt_topic.unwrap_or("map-invertor/1".into());
            let mut count = 0;
            let mut consecutive_same_reads = 0;

            let max_consecutive_reads = 300 / Duration::from(&interval).as_secs();

            let mut prev_map_info = MapInfo::default();
            loop {
                let map_info = map_protocol.read_status(&eeprom)?;
                // let map_info = MapInfo::default();
                if prev_map_info != map_info {
                    let msg =
                        Message::new_retained(topic, serde_json::to_vec(&map_info).unwrap(), QOS_1);
                    cli.publish(msg)?;
                    prev_map_info = map_info;
                    consecutive_same_reads = 0;
                } else {
                    consecutive_same_reads += 1;
                }
                count = (count + 1) % 10;
                trace!("map info: {:?}", &prev_map_info);
                trace!("count: {}", count);
                trace!("consecutive_same_reads: {}", consecutive_same_reads);
                if consecutive_same_reads > max_consecutive_reads {
                    warn!(
                        "map info not changed for {} seconds and {} iterations",
                        Duration::from(&interval).as_secs() * count,
                        count
                    );

                    bail!(
                        "map info not changed for {} seconds and {} iterations, restarting",
                        Duration::from(&interval).as_secs() * count,
                        count
                    );
                }

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
