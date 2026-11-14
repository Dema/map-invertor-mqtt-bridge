//! The command line surface: one argument group per working mode.

use clap::{Args, Command, CommandFactory, FromArgMatches, Parser, Subcommand};
use clap_complete::{generate, Generator, Shell};
use clap_duration::duration_range_value_parse;
use duration_human::{DurationHuman, DurationHumanValidator};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: WorkingMode,
}

#[derive(Clone, Debug, Args)]
pub struct SerialParams {
    /// Map port
    #[arg(short = 'p', long, env)]
    pub map_port: String,
    /// Map port speed
    #[arg(short = 's', long, env)]
    pub map_port_speed: u32,
}

#[derive(Clone, Debug, Args)]
pub struct MqttParams {
    #[command(flatten)]
    pub serial_params: SerialParams,
    /// MQTT broker hostname
    #[arg(short, long, env)]
    pub mqtt_hostname: String,
    /// MQTT broker port
    #[arg(long, env)]
    pub mqtt_port: u16,
    /// MQTT broker username
    #[arg(long, env)]
    pub mqtt_username: String,
    /// MQTT broker password
    #[arg(long, env)]
    pub mqtt_password: String,
    /// MQTT broker topic
    #[arg(long, env)]
    pub mqtt_topic: Option<String>,
    /// my id, default is "map-invertor-mqtt-bridge"
    #[arg(long, env)]
    pub mqtt_id: Option<String>,
    /// Polling interval
    #[arg(
        long, default_value="10s",
        value_parser = duration_range_value_parse!(min: 1s, max: 10min)
    )]
    pub interval: DurationHuman,
    /// Interval betweeen attempts to reconnect to MAP or MQTT server.
    /// Default is to not reconnect
    #[arg(
            long,value_parser = duration_range_value_parse!(min: 1s, max: 10min)
        )]
    pub reconnect_every: Option<DurationHuman>,
}

#[derive(Clone, Debug, Subcommand)]
pub enum WorkingMode {
    Mqtt(MqttParams),
    Stdout {
        #[command(flatten)]
        serial_params: SerialParams,
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

/// The arguments of one run, parsed as [`Cli`] renders them.
///
/// The help text, the completions and the parse have to agree on one command,
/// so they all come from [`Cli`] rather than from a second, hand-built one.
pub fn parse() -> Cli {
    let matches = command().get_matches();
    match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            eprint!("{}", error.render());
            std::process::exit(1);
        }
    }
}

/// The command a completion script is generated for.
pub fn command() -> Command {
    Cli::command()
}

pub fn print_completions<G: Generator>(gen: G, cmd: &mut Command) {
    generate(gen, cmd, cmd.get_name().to_string(), &mut std::io::stdout());
}

#[test]
fn verify_cli() {
    Cli::command().debug_assert()
}
