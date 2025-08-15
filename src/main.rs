use log::{self, info, warn};
use std::{thread, time::Duration};

use anyhow::bail;
use clap::{Args, Command, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_duration::duration_range_value_parse;
use duration_human::{DurationHuman, DurationHumanValidator};
use map_protocol::{
    high_level::{HighLevelProtocol, MapInfo},
    MapError,
};

use paho_mqtt::{Message, QOS_1};

mod map_protocol;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    mode: WorkingMode,
}
#[derive(Clone, Debug, Args)]

struct MqttParams {
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
    /// Interval betweeen attempts to reconnect to MAP or MQTT server.
    /// Default is to not reconnect
    #[arg(
            long,value_parser = duration_range_value_parse!(min: 1s, max: 10min)
        )]
    reconnect_every: Option<DurationHuman>,
}
#[derive(Clone, Debug, Subcommand)]
enum WorkingMode {
    Mqtt(MqttParams),
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
            let mut map_protocol = HighLevelProtocol::new(port)?;

            let eeprom = map_protocol.read_eeprom()?;
            if eeprom[0] != 3 {
                bail!("MAP not found");
            }
            let map_info = map_protocol.read_status(&eeprom)?;
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
        WorkingMode::Mqtt(params) => {
            if params.reconnect_every.is_some() {
                loop {
                    let result = mqtt_run_one_cycle(&params);
                    if result.is_err() {
                        warn!("{:?}", result.err().unwrap());
                        thread::sleep(Duration::from(&params.reconnect_every.unwrap()));
                    }
                }
            } else {
                mqtt_run_one_cycle(&params)?
            }
        }
    }
    Ok(())
}

fn mqtt_run_one_cycle(mqtt_params: &MqttParams) -> Result<(), anyhow::Error> {
    let port = serialport::new(&mqtt_params.map_port, mqtt_params.map_port_speed)
        .timeout(Duration::from_secs(20))
        .open()?;
    info!("Map port {} opened", mqtt_params.map_port);
    let mut map_protocol = HighLevelProtocol::new(port)?;
    let mqtt_id = mqtt_params
        .mqtt_id
        .as_deref()
        .unwrap_or("map-invertor-mqtt-bridge");
    // .unwrap_or("map-invertor-mqtt-bridge".into());
    let url: String = format!(
        "tcp://{}:{}",
        mqtt_params.mqtt_hostname, mqtt_params.mqtt_port
    );
    let cli = paho_mqtt::Client::new((url.clone(), mqtt_id.to_string()))?;
    let conn_opts = paho_mqtt::ConnectOptionsBuilder::new()
        .keep_alive_interval(Duration::from_secs(20))
        .user_name(&mqtt_params.mqtt_username)
        .password(&mqtt_params.mqtt_password)
        .clean_session(true)
        .finalize();
    cli.connect(conn_opts)?;

    info!("connected to MQTT broker at {}", url);
    let eeprom = map_protocol.read_eeprom()?;
    if eeprom[0] != 3 {
        return Err(MapError::NotFound.into());
    }

    let topic = mqtt_params
        .mqtt_topic
        .as_deref()
        .unwrap_or("map-invertor/1");
    let mut reads_count = 0;
    let mut consecutive_same_reads = 0;
    let interval = Duration::from(&mqtt_params.interval);
    let interval_in_seconds = interval.as_secs();
    let max_consecutive_reads = 300 / interval_in_seconds;
    let mut prev_map_info = MapInfo::default();
    loop {
        let map_info = map_protocol.read_status(&eeprom)?;
        if prev_map_info != map_info {
            let msg = Message::new_retained(topic, serde_json::to_vec(&map_info).unwrap(), QOS_1);
            cli.publish(msg)?;
            prev_map_info = map_info;
            consecutive_same_reads = 0;
        } else {
            consecutive_same_reads += 1;
        }
        reads_count = (reads_count + 1) % 10;
        if consecutive_same_reads > max_consecutive_reads {
            warn!(
                "map info not changed for {} seconds and {} iterations",
                interval_in_seconds * reads_count,
                reads_count
            );

            bail!(
                "map info not changed for {} seconds and {} iterations, restarting",
                interval_in_seconds * reads_count,
                reads_count
            );
        }

        thread::sleep(interval);
    }
}

#[test]
fn verify_cli() {
    use clap::CommandFactory;
    Cli::command().debug_assert()
}
