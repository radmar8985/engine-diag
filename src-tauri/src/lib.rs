use chrono::{DateTime, FixedOffset};
use influxdb2::models::Query;
use influxdb2::{Client, FromDataPoint};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Deserialize, Clone, Default, Debug, FromDataPoint)]
struct EngineData {
    voltage1: f64,
    voltage2: f64,
    current1: f64,
    current2: f64,
    temperature: f64,
    rpm: f64,
    power: f64,
    time: DateTime<FixedOffset>,
}

fn start_influx_stream(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let host = std::env::var("INFLUX_URL").expect("Brak INFLUX_URL w .env");

        let org = std::env::var("INFLUX_ORG").expect("Brak INFLUX_ORG w .env");

        let token = std::env::var("INFLUX_TOKEN").expect("Brak INFLUX_TOKEN w .env");

        let bucket = std::env::var("INFLUX_BUCKET").expect("Brak INFLUX_BUCKET w .env");

        let client = Client::new(host, org, token);
        
let query_string = format!(
    r#"
from(bucket: "{}")
    |> range(start: -1h)
    |> filter(fn: (r) => r._measurement == "sensor_readings")
    |> last()
    |> toFloat()
"#,
    bucket
);
        loop {
            let query = Query::new(query_string.clone());

            let result = client.query::<EngineData>(Some(query)).await;

            match result {
                Ok(records) => {
                    if let Some(data) = records.into_iter().next() {
                        println!("InfluxDB data: {:?}", data);

                        let _ = app_handle.emit("engine-data-update", data);
                    }
                }

                Err(e) => {
                    eprintln!("InfluxDB query error: {:?}", e);
                }
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();

            start_influx_stream(app_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
