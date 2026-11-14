//! The production sink: the inverter status as a retained MQTT message.

use std::time::Duration;

use log::info;
use paho_mqtt::{Client, ConnectOptionsBuilder, Message, QOS_1};
use serde_json::Error as JsonError;
use thiserror::Error;

use crate::{bridge::StatusSink, cli::MqttParams, map_protocol::high_level::MapInfo};

const DEFAULT_CLIENT_ID: &str = "map-invertor-mqtt-bridge";
/// Topic Home Assistant is subscribed to unless the run overrides it.
const DEFAULT_TOPIC: &str = "map-invertor/1";
const KEEP_ALIVE: Duration = Duration::from_secs(20);

#[derive(Debug, Error)]
pub enum SinkError {
    #[error("cannot serialize the MAP status: {0}")]
    Serialize(#[from] JsonError),
}

/// Publishes every status as retained JSON on one topic, at QoS 1.
///
/// Retained, because a dashboard that connects later still has to see the last
/// known status; QoS 1, because losing an update silently is worse than
/// receiving it twice.
pub struct MqttSink {
    client: Client,
    topic: String,
}

impl MqttSink {
    /// Builds a session and connects it to the configured broker.
    pub fn connect(params: &MqttParams) -> anyhow::Result<Self> {
        let url = format!("tcp://{}:{}", params.mqtt_hostname, params.mqtt_port);
        let client_id = params.mqtt_id.as_deref().unwrap_or(DEFAULT_CLIENT_ID);
        let client = Client::new((url.clone(), client_id.to_string()))?;
        let connect_options = ConnectOptionsBuilder::new()
            .keep_alive_interval(KEEP_ALIVE)
            .user_name(&params.mqtt_username)
            .password(&params.mqtt_password)
            .clean_session(true)
            .finalize();
        client.connect(connect_options)?;
        info!("connected to MQTT broker at {}", url);

        Ok(Self {
            client,
            topic: params
                .mqtt_topic
                .clone()
                .unwrap_or_else(|| DEFAULT_TOPIC.to_string()),
        })
    }
}

impl StatusSink for MqttSink {
    fn publish(&mut self, status: &MapInfo) -> anyhow::Result<()> {
        self.client.publish(status_message(&self.topic, status)?)?;
        Ok(())
    }
}

/// The one MQTT message a status becomes.
fn status_message(topic: &str, status: &MapInfo) -> Result<Message, SinkError> {
    let payload = serde_json::to_vec(status)?;
    Ok(Message::new_retained(topic, payload, QOS_1))
}

#[cfg(test)]
mod tests {
    use paho_mqtt::QOS_1;

    use crate::map_protocol::high_level::MapInfo;

    use super::{status_message, DEFAULT_TOPIC};

    #[test]
    fn a_status_becomes_a_retained_qos_1_json_message() {
        let message = status_message(DEFAULT_TOPIC, &MapInfo::for_test(1)).expect("serializable");

        assert_eq!(message.topic(), "map-invertor/1");
        assert!(message.retained(), "a late subscriber needs the status");
        assert_eq!(message.qos(), QOS_1.into());
        let payload: serde_json::Value =
            serde_json::from_slice(message.payload()).expect("valid JSON");
        assert_eq!(payload["status_char"], 1);
    }
}
