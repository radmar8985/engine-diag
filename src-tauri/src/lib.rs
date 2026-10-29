use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio_postgres::NoTls;

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
struct EngineData {
    voltage1: f64,
    voltage2: f64,
    voltage3: f64,
    current1: f64,
    current2: f64,
    current3: f64,
    power1: f64,
    power2: f64,
    power3: f64,
    temperature: f64,
    rpm: f64,
}

fn start_pg_stream(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let pg_host = std::env::var("PG_HOST").unwrap_or_else(|_| "localhost".to_string());
        let pg_port = std::env::var("PG_PORT").unwrap_or_else(|_| "5432".to_string());
        let pg_user = std::env::var("PG_USER").unwrap_or_else(|_| "postgres".to_string());
        let pg_password = std::env::var("PG_PASSWORD").unwrap_or_else(|_| "admin".to_string());
        let pg_dbname = std::env::var("PG_DBNAME").unwrap_or_else(|_| "engine_db".to_string());

        let conn_string = format!(
            "host={} port={} user={} password={} dbname={}",
            pg_host, pg_port, pg_user, pg_password, pg_dbname
        );

        println!("Łączenie z PostgreSQL: host={} port={} user={} dbname={}", pg_host, pg_port, pg_user, pg_dbname);

        // Retry connection up to 10 times
        let client = loop {
            match tokio_postgres::connect(&conn_string, NoTls).await {
                Ok((client, connection)) => {
                    // Spawn the connection handler in the background
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            eprintln!("PostgreSQL connection error: {:?}", e);
                        }
                    });
                    println!("Połączono z PostgreSQL/TimescaleDB!");
                    break client;
                }
                Err(e) => {
                    eprintln!("Błąd połączenia z PostgreSQL: {:?}. Ponowna próba za 3s...", e);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        };

        loop {
            let result = client
                .query_opt(
                    "SELECT voltage1, voltage2, voltage3, current1, current2, current3, \
                            power1, power2, power3, temperature, rpm \
                     FROM sensor_readings \
                     ORDER BY time DESC \
                     LIMIT 1",
                    &[],
                )
                .await;

            match result {
                Ok(Some(row)) => {
                    let data = EngineData {
                        voltage1: row.get::<_, Option<f64>>("voltage1").unwrap_or(0.0),
                        voltage2: row.get::<_, Option<f64>>("voltage2").unwrap_or(0.0),
                        voltage3: row.get::<_, Option<f64>>("voltage3").unwrap_or(0.0),
                        current1: row.get::<_, Option<f64>>("current1").unwrap_or(0.0),
                        current2: row.get::<_, Option<f64>>("current2").unwrap_or(0.0),
                        current3: row.get::<_, Option<f64>>("current3").unwrap_or(0.0),
                        power1: row.get::<_, Option<f64>>("power1").unwrap_or(0.0),
                        power2: row.get::<_, Option<f64>>("power2").unwrap_or(0.0),
                        power3: row.get::<_, Option<f64>>("power3").unwrap_or(0.0),
                        temperature: row.get::<_, Option<f64>>("temperature").unwrap_or(0.0),
                        rpm: row.get::<_, Option<f64>>("rpm").unwrap_or(0.0),
                    };

                    println!("PostgreSQL data: {:?}", data);
                    let _ = app_handle.emit("engine-data-update", data);
                }
                Ok(None) => {
                    eprintln!("Brak danych w tabeli sensor_readings");
                }
                Err(e) => {
                    eprintln!("PostgreSQL query error: {:?}", e);
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

            start_pg_stream(app_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
