#![feature(gethostname, never_type)]
use std::{net::hostname, time::Duration};

use anyhow::{Context, Result};
use log::{debug, info};
use mpris::{Metadata, PlayerFinder};
use rumqttc::{AsyncClient, MqttOptions};
use tokio::task;

struct MetadataEq<'a>(&'a Metadata);

#[tokio::main]
async fn main() -> Result<!> {
    env_logger::init();

    let mut mqtt_opts = MqttOptions::new("mpris2mqtt-rs", "mqtt.chaosdorf.space", 1883);
    mqtt_opts.set_keep_alive(Duration::from_secs(5));

    let (mut client, mut eventloop) = AsyncClient::new(mqtt_opts, 10);
    client
        .publish("music/title", rumqttc::QoS::AtMostOnce, true, "N/A")
        .await?;

    task::spawn(async move {
        loop {
            let notification = eventloop.poll().await.unwrap();
            println!("Received = {:?}", notification);
        }
    });
    let mut old_metadata: Option<Metadata> = None;
    loop {
        let metadata = get_metadata()?;
        debug!("Got metadata: {:?}", metadata);

        // check if changed
        if old_metadata
            .as_ref()
            .map(|old_md| MetadataEq(&metadata) == MetadataEq(old_md))
            .unwrap_or(false)
        {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        let res = publish_mqtt(&mut client, &metadata).await?;
        info!("Published metadata to MQTT: {:?}", res);

        old_metadata = Some(metadata);

        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn get_metadata() -> Result<Metadata> {
    let player_finder = PlayerFinder::new().context("Could not connect to D-Bus")?;

    let player = player_finder
        .find_active()
        .context("Could not find any player")?;

    println!(
        "Found {identity} (on bus {bus_name})",
        bus_name = player.bus_name(),
        identity = player.identity(),
    );

    player
        .get_metadata()
        .context("Could not get metadata for player")
}

async fn publish_mqtt(client: &mut AsyncClient, md: &Metadata) -> Result<()> {
    client
        .publish(
            "music/title",
            rumqttc::QoS::AtMostOnce,
            true,
            md.title().unwrap_or("N/A"),
        )
        .await?;
    debug!("Published title: {:?}", md.title());
    client
        .publish(
            "music/artist",
            rumqttc::QoS::AtMostOnce,
            true,
            md.artists()
                .map(|v| v.join(", "))
                .as_deref()
                .unwrap_or("N/A"),
        )
        .await?;
    debug!("Published artist: {:?}", md.artists());

    client
        .publish(
            "music/album",
            rumqttc::QoS::AtMostOnce,
            true,
            md.album_name().unwrap_or("N/A"),
        )
        .await?;
    debug!("Published album: {:?}", md.album_name());

    let hostname = hostname()?.into_string();
    if let Ok(hostname_str) = hostname {
        client
            .publish(
                "music/source",
                rumqttc::QoS::AtMostOnce,
                true,
                hostname_str.clone(),
            )
            .await?;
        debug!("Published source: {:?}", &hostname_str);
    }

    Ok(())
}

impl PartialEq for MetadataEq<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.title() == other.0.title()
            && self.0.artists() == other.0.artists()
            && self.0.album_name() == other.0.album_name()
    }
}
