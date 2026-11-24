pub mod pcan;

use pcan::{is_status_hardware_ok, PcanDriver, PCAN_BAUD_500K};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tokio_postgres::NoTls;

// ── Data models ──

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct EngineData {
    pub voltage1: f64,
    pub voltage2: f64,
    pub voltage3: f64,
    pub current1: f64,
    pub current2: f64,
    pub current3: f64,
    pub power1: f64,
    pub power2: f64,
    pub power3: f64,
    pub temperature: f64,
    pub rpm: f64,
}

/// CAN data received from the bus — maps to DBC signals
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct CanReadData {
    // MCU_ANALOG_FUEL_CELL (0x104)
    pub fc_output_voltage: f64,
    pub fc_output_current: f64,
    pub h2_high_pressure: f64,
    // MCU_ANALOG_POWERTRAIN (0x105)
    pub sc_voltage: f64,
    pub sc_current: f64,
    pub mc_supply_voltage: f64,
    pub mc_supply_current: f64,
    // MCU_ANALOG_ACCESSORY (0x106)
    pub ab_voltage: f64,
    pub ab_current: f64,
    // MCU_ANALOG_DRIVE (0x107)
    pub rpm: f64,
    pub speed: f64,
    // FCCU_POWER (0x503)
    pub fccu_fc_voltage: f64,
    pub fccu_fc_current: f64,
    pub fccu_dcdc_voltage: f64,
    pub fccu_dcdc_current: f64,
    // FCCU_ENVIRONMENT (0x504)
    pub fc_temp: f64,
    pub ambient_temp: f64,
    pub ambient_humidity: f64,
    // FCCU_STATE (0x500)
    pub fccu_state: u8,
    pub fccu_main_valve: bool,
    pub fccu_purge_valve: bool,
    pub fccu_fan_state: bool,
    pub fccu_fan_duty: u8,
    // FCCU_VERSION (0x501)
    pub fccu_version: String,
    // FCCU_HYDROGEN (0x502)
    pub h2_flow: f64,
    pub h2_lp_pressure: f64,
    // PROTIUM_POWER (0x200)
    pub fc_w: f64,
    pub energy_wh: f64,
    // MCU_STATE (0x100)
    pub mcu_status: u8,
    pub vehicle_type: u8,
    pub motor_controller_enabled: bool,
    pub main_valve_enabled: bool,
}

// ── Shared PCAN state ──

pub struct CanState(pub Arc<RwLock<Option<Arc<PcanDriver>>>>);

// ── PostgreSQL configuration & state ──

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub dbname: String,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: "admin".to_string(),
            dbname: "engine_db".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct DbStatus {
    pub connected: bool,
    pub message: String,
}

pub struct DbConfigState {
    pub tx: tokio::sync::watch::Sender<DbConfig>,
    pub status: Arc<RwLock<DbStatus>>,
}

pub fn load_db_config_from_path(path: &std::path::Path) -> DbConfig {
    let mut cfg = DbConfig::default();
    if let Ok(iter) = dotenvy::from_path_iter(path) {
        for item in iter.flatten() {
            match item.0.as_str() {
                "PG_HOST" => cfg.host = item.1,
                "PG_PORT" => {
                    if let Ok(p) = item.1.parse::<u16>() {
                        cfg.port = p;
                    }
                }
                "PG_USER" => cfg.user = item.1,
                "PG_PASSWORD" => cfg.password = item.1,
                "PG_DBNAME" => cfg.dbname = item.1,
                _ => {}
            }
        }
    }
    cfg
}

fn load_initial_db_config() -> DbConfig {
    dotenvy::dotenv().ok();
    DbConfig {
        host: std::env::var("PG_HOST").unwrap_or_else(|_| "localhost".to_string()),
        port: std::env::var("PG_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(5432),
        user: std::env::var("PG_USER").unwrap_or_else(|_| "postgres".to_string()),
        password: std::env::var("PG_PASSWORD").unwrap_or_else(|_| "admin".to_string()),
        dbname: std::env::var("PG_DBNAME").unwrap_or_else(|_| "engine_db".to_string()),
    }
}

pub fn save_db_config_to_path(config: &DbConfig, path: &std::path::Path) -> Result<(), String> {
    let content = format!(
        "PG_HOST={}\nPG_PORT={}\nPG_USER={}\nPG_PASSWORD={}\nPG_DBNAME={}\n",
        config.host, config.port, config.user, config.password, config.dbname
    );
    std::fs::write(path, content).map_err(|e| format!("Błąd zapisu pliku .env: {}", e))
}

fn save_db_config_to_env(config: &DbConfig) -> Result<(), String> {
    std::env::set_var("PG_HOST", &config.host);
    std::env::set_var("PG_PORT", config.port.to_string());
    std::env::set_var("PG_USER", &config.user);
    std::env::set_var("PG_PASSWORD", &config.password);
    std::env::set_var("PG_DBNAME", &config.dbname);

    let paths = [
        std::path::PathBuf::from(".env"),
        std::path::PathBuf::from("../.env"),
    ];
    let mut written = false;
    for p in &paths {
        if p.exists() && save_db_config_to_path(config, p).is_ok() {
            written = true;
        }
    }
    if !written {
        save_db_config_to_path(config, std::path::Path::new(".env"))?;
    }
    Ok(())
}

// ── PostgreSQL stream (Grafana tab) ──

fn start_pg_stream(
    app_handle: AppHandle,
    mut config_rx: tokio::sync::watch::Receiver<DbConfig>,
    status_arc: Arc<RwLock<DbStatus>>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let current_config = config_rx.borrow_and_update().clone();
            let conn_string = format!(
                "host={} port={} user={} password={} dbname={}",
                current_config.host,
                current_config.port,
                current_config.user,
                current_config.password,
                current_config.dbname
            );

            let status_connecting = DbStatus {
                connected: false,
                message: format!("Łączenie z {}:{}...", current_config.host, current_config.port),
            };
            if let Ok(mut s) = status_arc.write() {
                *s = status_connecting.clone();
            }
            let _ = app_handle.emit("db-connection-status", status_connecting);

            println!(
                "Łączenie z PostgreSQL: host={} port={} user={} dbname={}",
                current_config.host, current_config.port, current_config.user, current_config.dbname
            );

            let connect_result = tokio::select! {
                res = tokio_postgres::connect(&conn_string, NoTls) => Some(res),
                _ = config_rx.changed() => None,
            };

            let client = match connect_result {
                Some(Ok((client, connection))) => {
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            eprintln!("PostgreSQL connection error: {:?}", e);
                        }
                    });
                    println!("Połączono z PostgreSQL/TimescaleDB!");
                    let status_ok = DbStatus {
                        connected: true,
                        message: format!("Połączono ({})", current_config.dbname),
                    };
                    if let Ok(mut s) = status_arc.write() {
                        *s = status_ok.clone();
                    }
                    let _ = app_handle.emit("db-connection-status", status_ok);
                    client
                }
                Some(Err(e)) => {
                    let err_msg = format!("Błąd połączenia: {}", e);
                    eprintln!("Błąd połączenia z PostgreSQL: {}. Ponowna próba za 3s...", e);
                    let status_err = DbStatus {
                        connected: false,
                        message: err_msg,
                    };
                    if let Ok(mut s) = status_arc.write() {
                        *s = status_err.clone();
                    }
                    let _ = app_handle.emit("db-connection-status", status_err);

                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                        _ = config_rx.changed() => {}
                    }
                    continue;
                }
                None => {
                    continue;
                }
            };

            let mut is_connected = true;
            while is_connected {
                tokio::select! {
                    _ = config_rx.changed() => {
                        println!("Wykryto zmianę konfiguracji bazy danych, zamykanie obecnego połączenia...");
                        is_connected = false;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
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

                                let _ = app_handle.emit("engine-data-update", data);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                eprintln!("PostgreSQL query error: {:?}", e);
                                let status_err = DbStatus {
                                    connected: false,
                                    message: format!("Błąd zapytania: {}", e),
                                };
                                if let Ok(mut s) = status_arc.write() {
                                    *s = status_err.clone();
                                }
                                let _ = app_handle.emit("db-connection-status", status_err);
                                is_connected = false;
                            }
                        }
                    }
                }
            }
        }
    });
}

// ── CAN frame decoder with DBC parser ──

/// Load or parse the vehicle DBC definition using can_decode::Parser.
pub fn create_dbc_parser() -> can_decode::Parser {
    let mut parser = can_decode::Parser::new();
    let loaded = if let Ok(path) = std::env::var("DBC_FILE_PATH") {
        parser.add_from_dbc_file(std::path::Path::new(&path)).is_ok()
    } else if std::path::Path::new("can/vehicle.dbc").exists() {
        parser.add_from_dbc_file(std::path::Path::new("can/vehicle.dbc")).is_ok()
    } else if std::path::Path::new("src-tauri/can/vehicle.dbc").exists() {
        parser.add_from_dbc_file(std::path::Path::new("src-tauri/can/vehicle.dbc")).is_ok()
    } else {
        false
    };

    if !loaded {
        const EMBEDDED_DBC: &str = include_str!("../can/vehicle.dbc");
        if let Err(e) = parser.add_from_str(EMBEDDED_DBC) {
            eprintln!("[DBC] Błąd ładowania wbudowanego pliku DBC: {:?}", e);
        } else {
            println!(
                "[DBC] Załadowano wbudowany plik DBC ({} definicji wiadomości)",
                parser.msg_defs().len()
            );
        }
    } else {
        println!(
            "[DBC] Załadowano plik DBC z dysku ({} definicji wiadomości)",
            parser.msg_defs().len()
        );
    }

    parser
}

/// Decode a single CAN frame into the aggregated CanReadData struct using DBC definitions.
/// Automatically handles bit unpacking, byte order, signed values, scaling factors, and offsets.
pub fn decode_can_frame(
    parser: &can_decode::Parser,
    data: &mut CanReadData,
    id: u32,
    payload: &[u8],
) {
    let mut buf = [0u8; 8];
    let len = payload.len().min(8);
    buf[..len].copy_from_slice(&payload[..len]);

    // Check with standard ID, or extended ID (bit 31 set as per DBC extended ID convention)
    let decoded_msg = parser
        .decode_msg(id, &buf)
        .or_else(|| parser.decode_msg(id | (1 << 31), &buf));

    let Some(msg) = decoded_msg else {
        return;
    };

    match msg.name.as_str() {
        "MCU_STATE" => {
            if let Some(s) = msg.signals.get("STATUS") {
                data.mcu_status = s.value.raw.unwrap_or(s.value.physical as i128) as u8;
            }
            if let Some(s) = msg.signals.get("VEHICLE_TYPE") {
                data.vehicle_type = s.value.raw.unwrap_or(s.value.physical as i128) as u8;
            }
            if let Some(s) = msg.signals.get("MOTOR_CONTROLLER_ENABLED") {
                data.motor_controller_enabled = s.value.physical > 0.0;
            }
            if let Some(s) = msg.signals.get("MAIN_VALVE_ENABLED") {
                data.main_valve_enabled = s.value.physical > 0.0;
            }
        }
        "MCU_ANALOG_FUEL_CELL" => {
            if let Some(s) = msg.signals.get("FUEL_CELL_OUTPUT_CURRENT") {
                data.fc_output_current = s.value.physical;
            }
            if let Some(s) = msg.signals.get("FUEL_CELL_OUTPUT_VOLTAGE") {
                data.fc_output_voltage = s.value.physical;
            }
            if let Some(s) = msg.signals.get("HYDROGEN_HIGH_PRESSURE") {
                data.h2_high_pressure = s.value.physical;
            }
        }
        "MCU_ANALOG_POWERTRAIN" => {
            if let Some(s) = msg.signals.get("SUPERCAPACITOR_VOLTAGE") {
                data.sc_voltage = s.value.physical;
            }
            if let Some(s) = msg.signals.get("SUPERCAPACITOR_CURRENT") {
                data.sc_current = s.value.physical;
            }
            if let Some(s) = msg.signals.get("MOTOR_CONTROLLER_SUPPLY_CURRENT") {
                data.mc_supply_current = s.value.physical;
            }
            if let Some(s) = msg.signals.get("MOTOR_CONTROLLER_SUPPLY_VOLTAGE") {
                data.mc_supply_voltage = s.value.physical;
            }
        }
        "MCU_ANALOG_ACCESSORY" => {
            if let Some(s) = msg.signals.get("ACCESSORY_BATTERY_CURRENT") {
                data.ab_current = s.value.physical;
            }
            if let Some(s) = msg.signals.get("ACCESSORY_BATTERY_VOLTAGE") {
                data.ab_voltage = s.value.physical;
            }
        }
        "MCU_ANALOG_DRIVE" => {
            if let Some(s) = msg.signals.get("RPM") {
                data.rpm = s.value.physical;
            }
            if let Some(s) = msg.signals.get("SPEED") {
                data.speed = s.value.physical;
            }
        }
        "PROTIUM_POWER" => {
            if let Some(s) = msg.signals.get("FC_V") {
                data.fc_output_voltage = s.value.physical;
            }
            if let Some(s) = msg.signals.get("FC_A") {
                data.fc_output_current = s.value.physical;
            }
            if let Some(s) = msg.signals.get("FC_W") {
                data.fc_w = s.value.physical;
            }
            if let Some(s) = msg.signals.get("ENERGY") {
                data.energy_wh = s.value.physical;
            }
        }
        "FCCU_STATE" => {
            if let Some(s) = msg.signals.get("STATE") {
                data.fccu_state = s.value.raw.unwrap_or(s.value.physical as i128) as u8;
            }
            if let Some(s) = msg.signals.get("MAIN_VALVE") {
                data.fccu_main_valve = s.value.physical > 0.0;
            }
            if let Some(s) = msg.signals.get("PURGE_VALVE") {
                data.fccu_purge_valve = s.value.physical > 0.0;
            }
            if let Some(s) = msg.signals.get("FAN_STATE") {
                data.fccu_fan_state = s.value.physical > 0.0;
            }
            if let Some(s) = msg.signals.get("FAN_DUTY") {
                data.fccu_fan_duty = s.value.raw.unwrap_or(s.value.physical as i128) as u8;
            }
        }
        "FCCU_VERSION" => {
            let major = msg
                .signals
                .get("FIRMWARE_MAJOR")
                .map(|s| s.value.physical as u8)
                .unwrap_or(0);
            let minor = msg
                .signals
                .get("FIRMWARE_MINOR")
                .map(|s| s.value.physical as u8)
                .unwrap_or(0);
            let patch = msg
                .signals
                .get("FIRMWARE_PATCH")
                .map(|s| s.value.physical as u8)
                .unwrap_or(0);
            data.fccu_version = format!("v{}.{}.{}", major, minor, patch);
        }
        "FCCU_HYDROGEN" => {
            if let Some(s) = msg.signals.get("H2_FLOW") {
                data.h2_flow = s.value.physical;
            }
            if let Some(s) = msg.signals.get("H2_LP_PRESSURE") {
                data.h2_lp_pressure = s.value.physical;
            }
        }
        "FCCU_POWER" => {
            if let Some(s) = msg.signals.get("FC_VOLTAGE") {
                data.fccu_fc_voltage = s.value.physical;
                data.fc_output_voltage = s.value.physical;
            }
            if let Some(s) = msg.signals.get("FC_CURRENT") {
                data.fccu_fc_current = s.value.physical;
                data.fc_output_current = s.value.physical;
            }
            data.fc_w = data.fc_output_voltage * data.fc_output_current;
            if let Some(s) = msg.signals.get("DCDC_VOLTAGE") {
                data.fccu_dcdc_voltage = s.value.physical;
            }
            if let Some(s) = msg.signals.get("DCDC_CURRENT") {
                data.fccu_dcdc_current = s.value.physical;
            }
        }
        "FCCU_ENVIRONMENT" => {
            if let Some(s) = msg.signals.get("FC_TEMP_C") {
                data.fc_temp = s.value.physical;
            }
            if let Some(s) = msg.signals.get("AMBIENT_TEMP_C") {
                data.ambient_temp = s.value.physical;
            }
            if let Some(s) = msg.signals.get("AMBIENT_HUMIDITY") {
                data.ambient_humidity = s.value.physical;
            }
        }
        _ => {}
    }
}

// ── CAN receive stream (CAN tab) ──

/// Parses PCAN channel specification (e.g. "1" -> 0x51, "0x52" -> 0x52).
pub fn parse_pcan_channel(v: &str) -> Option<u16> {
    let trimmed = v.trim();
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        u16::from_str_radix(&trimmed[2..], 16).ok()
    } else if let Ok(num) = trimmed.parse::<u16>() {
        if num == 0 {
            None
        } else if num < 0x50 {
            Some(0x50 + num) // e.g. "1" -> 0x51, "2" -> 0x52
        } else {
            Some(num)
        }
    } else {
        None
    }
}

fn get_preferred_pcan_channel() -> Option<u16> {
    std::env::var("PCAN_CHANNEL")
        .ok()
        .and_then(|v| parse_pcan_channel(&v))
}

fn start_can_stream(app_handle: AppHandle, pcan_holder: Arc<RwLock<Option<Arc<PcanDriver>>>>) {
    tauri::async_runtime::spawn(async move {
        println!("[CAN] Pętla obsługi magistrali CAN uruchomiona (ciągłe nasłuchiwanie / auto-reconnect)");

        let parser = create_dbc_parser();
        let mut accumulated = CanReadData::default();
        let mut was_connected = false;

        loop {
            let driver_opt = {
                pcan_holder.read().ok().and_then(|guard| guard.clone())
            };

            let driver = match driver_opt {
                Some(d) => {
                    if !d.is_hardware_ok() {
                        eprintln!("[CAN] Wykryto fizyczne odłączenie adaptera PCAN-USB (sprawdzenie sprzętowe)");
                        if let Ok(mut guard) = pcan_holder.write() {
                            *guard = None;
                        }
                        if was_connected {
                            let _ = app_handle.emit("can-connection-status", false);
                            was_connected = false;
                        }
                        accumulated = CanReadData::default();
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                    d
                }
                None => {
                    // Brak adaptera — ciągłe nasłuchiwanie USB w tle (auto-reconnect)
                    let preferred_channel = get_preferred_pcan_channel();
                    match PcanDriver::new_auto(preferred_channel, PCAN_BAUD_500K) {
                        Ok(new_driver) => {
                            let ch = new_driver.channel();
                            println!(
                                "[CAN] Automatycznie wykryto i połączono adapter PCAN-USB (kanał 0x{:02X})!",
                                ch
                            );
                            let arc_driver = Arc::new(new_driver);
                            if let Ok(mut guard) = pcan_holder.write() {
                                *guard = Some(arc_driver.clone());
                            }
                            let _ = app_handle.emit("can-connection-status", true);
                            was_connected = true;
                            arc_driver
                        }
                        Err(_) => {
                            if was_connected {
                                let _ = app_handle.emit("can-connection-status", false);
                                was_connected = false;
                            }
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            continue;
                        }
                    }
                }
            };

            if !was_connected {
                let _ = app_handle.emit("can-connection-status", true);
                was_connected = true;
            }

            let mut frames_read = 0u32;
            let mut hardware_lost = false;
            loop {
                match driver.read() {
                    Ok(Some(msg)) => {
                        let can_id = msg.id & 0x1FFFFFFF; // mask to 29-bit
                        let len = (msg.len as usize).min(msg.data.len());
                        decode_can_frame(
                            &parser,
                            &mut accumulated,
                            can_id,
                            &msg.data[..len],
                        );
                        frames_read += 1;
                        if frames_read >= 500 {
                            break;
                        }
                    }
                    Ok(None) => break, // buffer empty
                    Err(err) => {
                        if !is_status_hardware_ok(err) || !driver.is_hardware_ok() {
                            eprintln!("[CAN] Błąd odczytu 0x{:05X} — adapter odłączony!", err);
                            hardware_lost = true;
                        } else {
                            eprintln!("[CAN] Ostrzeżenie magistrali CAN: 0x{:05X}", err);
                        }
                        break;
                    }
                }
            }

            if hardware_lost {
                if let Ok(mut guard) = pcan_holder.write() {
                    *guard = None;
                }
                let _ = app_handle.emit("can-connection-status", false);
                was_connected = false;
                accumulated = CanReadData::default();
                tokio::time::sleep(Duration::from_millis(500)).await;
            } else {
                let _ = app_handle.emit("can-data-update", accumulated.clone());
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    });
}

// ── Pure encoding & validation helpers ──

/// Encodes an FCCU toggle command into `(can_id, message_name, payload)`.
pub fn encode_fccu_command(
    command_type: &str,
    state: bool,
) -> Result<(u32, &'static str, [u8; 1]), String> {
    let (can_id, name) = match command_type {
        "system_enable" => (0x510u32, "FCCU_SYSTEM_ENABLE"),
        "main_valve" => (0x511, "FCCU_MAIN_VALVE_COMMAND"),
        "manual_purge" => (0x512, "FCCU_MANUAL_PURGE"),
        "fan_on" => (0x513, "FCCU_FAN"),
        _ => return Err(format!("Nieznana komenda FCCU: {}", command_type)),
    };
    Ok((can_id, name, [state as u8]))
}

/// Encodes FCCU fan & purge valve settings into `(FCCU_FAN_SETTINGS 0x520, FCCU_VALVE_SETTINGS 0x521)` payloads.
pub fn encode_fccu_settings(
    fan_mode: u8,
    purge_interval: u16,
    p_gain: u16,
    i_gain: u16,
    d_gain: u16,
) -> ([u8; 8], [u8; 4]) {
    let fan_data: [u8; 8] = [
        fan_mode & 0x03,
        0x00,
        (p_gain & 0xFF) as u8,
        ((p_gain >> 8) & 0xFF) as u8,
        (i_gain & 0xFF) as u8,
        ((i_gain >> 8) & 0xFF) as u8,
        (d_gain & 0xFF) as u8,
        ((d_gain >> 8) & 0xFF) as u8,
    ];

    let valve_data: [u8; 4] = [
        (purge_interval & 0xFF) as u8,
        ((purge_interval >> 8) & 0xFF) as u8,
        100u8,
        0x00,
    ];

    (fan_data, valve_data)
}

/// Encodes vehicle lighting flags into the `MCU_LIGHTING (0x400)` single-byte payload.
pub fn encode_lighting(
    headlight: bool,
    position_light: bool,
    brake_light: bool,
    left_indicator: bool,
    right_indicator: bool,
    hazard: bool,
) -> u8 {
    (headlight as u8)
        | ((position_light as u8) << 1)
        | ((brake_light as u8) << 2)
        | ((left_indicator as u8) << 3)
        | ((right_indicator as u8) << 4)
        | ((hazard as u8) << 5)
}

/// Validates PostgreSQL connection settings before saving.
pub fn validate_db_config(config: &DbConfig) -> Result<(), String> {
    if config.host.trim().is_empty() {
        return Err("Host nie może być pusty".to_string());
    }
    if config.port == 0 {
        return Err("Port musi być w zakresie 1-65535".to_string());
    }
    if config.dbname.trim().is_empty() {
        return Err("Nazwa bazy danych nie może być pusta".to_string());
    }
    if config.user.trim().is_empty() {
        return Err("Użytkownik nie może być pusty".to_string());
    }
    Ok(())
}

/// Executes FCCU command transmission against shared `CanState`.
pub fn execute_fccu_command(
    command_type: &str,
    state: bool,
    can_state: &CanState,
) -> Result<String, String> {
    let (can_id, name, payload) = encode_fccu_command(command_type, state)?;
    println!(
        "[CAN TX] {} (0x{:08X}): [0x{:02X}] → {}",
        name,
        can_id,
        payload[0],
        if state { "ON" } else { "OFF" }
    );

    let driver_opt = can_state.0.read().ok().and_then(|guard| guard.clone());
    if let Some(ref driver) = driver_opt {
        driver.write(can_id, &payload).map_err(|e| {
            format!("CAN write error 0x{:05X} for {}", e, name)
        })?;
    } else {
        return Err("Brak adaptera PCAN-USB — ramka nie została wysłana".to_string());
    }

    Ok(format!(
        "{} → {}",
        name,
        if state { "ON" } else { "OFF" }
    ))
}

/// Executes FCCU fan & valve settings transmission against shared `CanState`.
pub fn execute_fccu_settings(
    fan_mode: u8,
    purge_interval: u16,
    p_gain: u16,
    i_gain: u16,
    d_gain: u16,
    can_state: &CanState,
) -> Result<String, String> {
    let (fan_data, valve_data) =
        encode_fccu_settings(fan_mode, purge_interval, p_gain, i_gain, d_gain);

    println!(
        "[CAN TX] FCCU_FAN_SETTINGS (0x520): {:02X?}  (fan_mode={}, P={}, I={}, D={})",
        fan_data, fan_mode, p_gain, i_gain, d_gain
    );
    println!(
        "[CAN TX] FCCU_VALVE_SETTINGS (0x521): {:02X?}  (purge_interval={}s, duration=100ms)",
        valve_data, purge_interval
    );

    let driver_opt = can_state.0.read().ok().and_then(|guard| guard.clone());
    if let Some(ref driver) = driver_opt {
        driver
            .write(0x520, &fan_data)
            .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_FAN_SETTINGS (0x520)", e))?;
        driver
            .write(0x521, &valve_data)
            .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_VALVE_SETTINGS (0x521)", e))?;
    } else {
        return Err("Brak adaptera PCAN-USB — ustawienia nie zostały wysłane".to_string());
    }

    Ok(format!(
        "Wysłano FCCU Settings: fan_mode={}, purge={}s, PID=({},{},{})",
        fan_mode, purge_interval, p_gain, i_gain, d_gain
    ))
}

/// Executes lighting frame transmission against shared `CanState`.
pub fn execute_lighting(
    headlight: bool,
    position_light: bool,
    brake_light: bool,
    left_indicator: bool,
    right_indicator: bool,
    hazard: bool,
    can_state: &CanState,
) -> Result<String, String> {
    let byte0 = encode_lighting(
        headlight,
        position_light,
        brake_light,
        left_indicator,
        right_indicator,
        hazard,
    );

    println!(
        "[CAN TX] MCU_LIGHTING (0x400): [0x{:02X}]  H={} P={} B={} L={} R={} Hz={}",
        byte0, headlight, position_light, brake_light, left_indicator, right_indicator, hazard
    );

    let driver_opt = can_state.0.read().ok().and_then(|guard| guard.clone());
    if let Some(ref driver) = driver_opt {
        driver
            .write(0x400, &[byte0])
            .map_err(|e| format!("CAN write error 0x{:05X} for MCU_LIGHTING", e))?;
    } else {
        return Err("Brak adaptera PCAN-USB — oświetlenie nie zostało wysłane".to_string());
    }

    Ok(format!(
        "Wysłano Lighting: H={} P={} B={} L={} R={} Hz={}",
        headlight, position_light, brake_light, left_indicator, right_indicator, hazard
    ))
}

/// Reads current CAN hardware connectivity from `CanState`.
pub fn read_can_status(can_state: &CanState) -> bool {
    can_state
        .0
        .read()
        .map(|guard| {
            guard
                .as_ref()
                .map(|driver| driver.is_hardware_ok())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Validates and propagates updated DB configuration into `DbConfigState` (and optionally a `.env` file path).
pub fn apply_db_config(
    config: DbConfig,
    state: &DbConfigState,
    env_path: Option<&std::path::Path>,
) -> Result<String, String> {
    validate_db_config(&config)?;
    match env_path {
        Some(path) => save_db_config_to_path(&config, path)?,
        None => save_db_config_to_env(&config)?,
    }
    let _ = state.tx.send(config);
    Ok("Dane zapisane pomyślnie. Trwa łączenie z bazą...".to_string())
}

// ── CAN send commands ──

/// Send a single FCCU command (instant toggle).
/// command_type: "system_enable" | "main_valve" | "manual_purge" | "fan_on"
/// state: true = ON (0x01), false = OFF (0x00)
#[tauri::command]
fn send_fccu_command(
    command_type: String,
    state: bool,
    can_state: State<'_, CanState>,
) -> Result<String, String> {
    execute_fccu_command(&command_type, state, &can_state)
}

/// Send FCCU settings
/// DBC messages: FCCU_FAN_SETTINGS (0x80000520 → 0x520) and FCCU_VALVE_SETTINGS (0x80000521 → 0x521)
#[tauri::command]
fn send_fccu_settings(
    fan_mode: u8,
    purge_interval: u16,
    p_gain: u16,
    i_gain: u16,
    d_gain: u16,
    can_state: State<'_, CanState>,
) -> Result<String, String> {
    execute_fccu_settings(
        fan_mode,
        purge_interval,
        p_gain,
        i_gain,
        d_gain,
        &can_state,
    )
}

/// Send MCU lighting commands
/// DBC message: MCU_LIGHTING (0x80000400 → 0x400)
#[tauri::command]
fn send_lighting(
    headlight: bool,
    position_light: bool,
    brake_light: bool,
    left_indicator: bool,
    right_indicator: bool,
    hazard: bool,
    can_state: State<'_, CanState>,
) -> Result<String, String> {
    execute_lighting(
        headlight,
        position_light,
        brake_light,
        left_indicator,
        right_indicator,
        hazard,
        &can_state,
    )
}

#[tauri::command]
fn get_can_status(can_state: State<'_, CanState>) -> bool {
    read_can_status(&can_state)
}

#[tauri::command]
async fn reconnect_pcan<R: Runtime>(
    app_handle: AppHandle<R>,
    can_state: State<'_, CanState>,
) -> Result<String, String> {
    let preferred_channel = get_preferred_pcan_channel();
    match PcanDriver::new_auto(preferred_channel, PCAN_BAUD_500K) {
        Ok(driver) => {
            let ch = driver.channel();
            let arc_driver = Arc::new(driver);
            if let Ok(mut guard) = can_state.0.write() {
                *guard = Some(arc_driver);
            }
            let _ = app_handle.emit("can-connection-status", true);
            Ok(format!("Połączono z adapterem PCAN-USB (kanał 0x{:02X})", ch))
        }
        Err(e) => {
            if let Ok(mut guard) = can_state.0.write() {
                *guard = None;
            }
            let _ = app_handle.emit("can-connection-status", false);
            Err(format!("Nie wykryto adaptera PCAN-USB: {}", e))
        }
    }
}

#[tauri::command]
fn get_db_config(state: State<'_, DbConfigState>) -> DbConfig {
    state.tx.borrow().clone()
}

#[tauri::command]
fn get_db_status(state: State<'_, DbConfigState>) -> DbStatus {
    state
        .status
        .read()
        .map(|s| s.clone())
        .unwrap_or(DbStatus {
            connected: false,
            message: "Nieznany".to_string(),
        })
}

#[tauri::command]
async fn save_db_config(
    config: DbConfig,
    state: State<'_, DbConfigState>,
) -> Result<String, String> {
    apply_db_config(config, &state, None)
}

/// Registers all application Tauri IPC command handlers onto any runtime builder (Wry or MockRuntime).
pub fn register_app_handlers<R: Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.invoke_handler(tauri::generate_handler![
        send_fccu_command,
        send_fccu_settings,
        send_lighting,
        get_db_config,
        get_db_status,
        save_db_config,
        get_can_status,
        reconnect_pcan,
    ])
}

// ── App entry point ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let initial_config = load_initial_db_config();
    let (config_tx, config_rx) = tokio::sync::watch::channel(initial_config);
    let db_status = Arc::new(RwLock::new(DbStatus {
        connected: false,
        message: "Inicjalizacja połączenia...".to_string(),
    }));

    let db_state = DbConfigState {
        tx: config_tx,
        status: db_status.clone(),
    };

    register_app_handlers(
        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .manage(db_state),
    )
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let can_handle = app.handle().clone();

            // Start PostgreSQL stream (Grafana tab)
            start_pg_stream(app_handle, config_rx, db_status);

            // Try to initialize PCAN-USB adapter (auto-detect free channel or read PCAN_CHANNEL env var)
            let preferred_channel = get_preferred_pcan_channel();

            let pcan_initial = match PcanDriver::new_auto(preferred_channel, PCAN_BAUD_500K) {
                Ok(driver) => {
                    println!(
                        "[PCAN] Adapter PCAN-USB (kanał 0x{:02X}) gotowy do pracy!",
                        driver.channel()
                    );
                    Some(Arc::new(driver))
                }
                Err(e) => {
                    eprintln!("[PCAN] Nie udało się zainicjalizować adaptera: {}", e);
                    eprintln!("[PCAN] Zakładka CAN będzie nieaktywna (brak odczytu/zapisu).");
                    None
                }
            };

            let pcan_holder = Arc::new(RwLock::new(pcan_initial));

            // Manage PCAN state for commands
            app.manage(CanState(pcan_holder.clone()));

            // Start CAN receive stream
            start_can_stream(can_handle, pcan_holder);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbc_parser_loading_and_decoding() {
        let parser = create_dbc_parser();
        assert!(!parser.msg_defs().is_empty(), "DBC parser should load messages");

        let mut data = CanReadData::default();

        // MCU_ANALOG_FUEL_CELL (0x104): current=15.2A, voltage=25.0V, pressure=300.0 bar
        let mut payload = [0u8; 8];
        payload[0..2].copy_from_slice(&15200i16.to_le_bytes());
        payload[2..4].copy_from_slice(&25000i16.to_le_bytes());
        payload[4..6].copy_from_slice(&3000u16.to_le_bytes());
        decode_can_frame(&parser, &mut data, 0x104, &payload);

        assert!((data.fc_output_current - 15.2).abs() < 1e-3);
        assert!((data.fc_output_voltage - 25.0).abs() < 1e-3);
        assert!((data.h2_high_pressure - 300.0).abs() < 1e-3);

        // FCCU_STATE (0x500): state=2 (RUNNING), main_valve=1, fan=1, fan_duty=75
        let state_byte = 2 | (1 << 4) | (1 << 6);
        let payload2 = [state_byte, 75u8, 0, 0, 0, 0, 0, 0];
        decode_can_frame(&parser, &mut data, 0x500, &payload2);

        assert_eq!(data.fccu_state, 2);
        assert!(data.fccu_main_valve);
        assert!(!data.fccu_purge_valve);
        assert!(data.fccu_fan_state);
        assert_eq!(data.fccu_fan_duty, 75);
    }

    #[test]
    fn test_decode_all_remaining_dbc_frames() {
        let parser = create_dbc_parser();
        let mut data = CanReadData::default();

        // MCU_STATE (0x100): status=3, vehicle_type=1, motor_controller_enabled (bit 14), main_valve_enabled (bit 15)
        let mcu_byte0 = (1u8 << 4) | 3u8;
        let mcu_byte1 = (1u8 << 6) | (1u8 << 7);
        decode_can_frame(&parser, &mut data, 0x100, &[mcu_byte0, mcu_byte1]);
        assert_eq!(data.mcu_status, 3);
        assert_eq!(data.vehicle_type, 1);
        assert!(data.motor_controller_enabled);
        assert!(data.main_valve_enabled);

        // MCU_ANALOG_POWERTRAIN (0x105)
        let mut pt = [0u8; 8];
        pt[0..2].copy_from_slice(&28500i16.to_le_bytes()); // SC V: 28.5 V
        pt[2..4].copy_from_slice(&1230i16.to_le_bytes());  // SC A: 12.3 A
        pt[4..6].copy_from_slice(&8400i16.to_le_bytes());  // MC A: 8.4 A
        pt[6..8].copy_from_slice(&31200i16.to_le_bytes()); // MC V: 31.2 V
        decode_can_frame(&parser, &mut data, 0x105, &pt);
        assert!((data.sc_voltage - 28.5).abs() < 1e-3);
        assert!((data.sc_current - 12.3).abs() < 1e-2);
        assert!((data.mc_supply_current - 8.4).abs() < 1e-3);
        assert!((data.mc_supply_voltage - 31.2).abs() < 1e-3);

        // MCU_ANALOG_ACCESSORY (0x106)
        let mut acc = [0u8; 4];
        acc[0..2].copy_from_slice(&2150i16.to_le_bytes());  // 2.15 A
        acc[2..4].copy_from_slice(&12600i16.to_le_bytes()); // 12.6 V
        decode_can_frame(&parser, &mut data, 0x106, &acc);
        assert!((data.ab_current - 2.15).abs() < 1e-3);
        assert!((data.ab_voltage - 12.6).abs() < 1e-3);

        // MCU_ANALOG_DRIVE (0x107)
        let mut drv = [0u8; 4];
        drv[0..2].copy_from_slice(&3200u16.to_le_bytes()); // 3200 rpm
        drv[2..4].copy_from_slice(&4250u16.to_le_bytes()); // 42.5 km/h
        decode_can_frame(&parser, &mut data, 0x107, &drv);
        assert!((data.rpm - 3200.0).abs() < 1e-3);
        assert!((data.speed - 42.5).abs() < 1e-2);

        // PROTIUM_POWER (0x200)
        let mut prot = [0u8; 8];
        prot[0..2].copy_from_slice(&4750i16.to_le_bytes()); // 47.5 V
        prot[2..4].copy_from_slice(&1050i16.to_le_bytes()); // 10.5 A
        prot[4..6].copy_from_slice(&499i16.to_le_bytes());  // 499 W
        prot[6..8].copy_from_slice(&120u16.to_le_bytes());  // 120 Wh
        decode_can_frame(&parser, &mut data, 0x200, &prot);
        assert!((data.fc_output_voltage - 47.5).abs() < 1e-2);
        assert!((data.fc_output_current - 10.5).abs() < 1e-2);
        assert!((data.fc_w - 499.0).abs() < 1e-3);
        assert!((data.energy_wh - 120.0).abs() < 1e-3);

        // FCCU_VERSION (0x501)
        decode_can_frame(&parser, &mut data, 0x501, &[2, 4, 15, 0, 0, 0, 0, 0]);
        assert_eq!(data.fccu_version, "v2.4.15");

        // FCCU_HYDROGEN (0x502)
        let mut h2 = [0u8; 8];
        h2[0..2].copy_from_slice(&1234u16.to_le_bytes()); // 12.34 Ln/min
        h2[2..4].copy_from_slice(&150u16.to_le_bytes());  // 1.50 bar
        decode_can_frame(&parser, &mut data, 0x502, &h2);
        assert!((data.h2_flow - 12.34).abs() < 1e-2);
        assert!((data.h2_lp_pressure - 1.5).abs() < 1e-2);

        // FCCU_POWER (0x503) — also calculates fc_w = V * A
        let mut fpow = [0u8; 8];
        fpow[0..2].copy_from_slice(&4500i16.to_le_bytes()); // 45.0 V
        fpow[2..4].copy_from_slice(&1000i16.to_le_bytes()); // 10.0 A
        fpow[4..6].copy_from_slice(&2400i16.to_le_bytes()); // 24.0 V
        fpow[6..8].copy_from_slice(&500i16.to_le_bytes());  // 5.0 A
        decode_can_frame(&parser, &mut data, 0x503, &fpow);
        assert!((data.fccu_fc_voltage - 45.0).abs() < 1e-2);
        assert!((data.fccu_fc_current - 10.0).abs() < 1e-2);
        assert!((data.fc_w - 450.0).abs() < 1e-2);
        assert!((data.fccu_dcdc_voltage - 24.0).abs() < 1e-2);
        assert!((data.fccu_dcdc_current - 5.0).abs() < 1e-2);

        // FCCU_ENVIRONMENT (0x504)
        let mut env = [0u8; 8];
        env[0..2].copy_from_slice(&5525i16.to_le_bytes()); // 55.25 C
        env[2..4].copy_from_slice(&2250i16.to_le_bytes()); // 22.50 C
        env[4..6].copy_from_slice(&4810u16.to_le_bytes()); // 48.10 %
        decode_can_frame(&parser, &mut data, 0x504, &env);
        assert!((data.fc_temp - 55.25).abs() < 1e-2);
        assert!((data.ambient_temp - 22.50).abs() < 1e-2);
        assert!((data.ambient_humidity - 48.10).abs() < 1e-2);

        // Unknown frame ID should leave data untouched
        let snapshot = data.clone();
        decode_can_frame(&parser, &mut data, 0x7FF, &[0xFF; 8]);
        assert_eq!(data, snapshot);
    }

    #[test]
    fn test_encode_fccu_commands() {
        assert_eq!(
            encode_fccu_command("system_enable", true).unwrap(),
            (0x510, "FCCU_SYSTEM_ENABLE", [0x01])
        );
        assert_eq!(
            encode_fccu_command("main_valve", false).unwrap(),
            (0x511, "FCCU_MAIN_VALVE_COMMAND", [0x00])
        );
        assert_eq!(
            encode_fccu_command("manual_purge", true).unwrap(),
            (0x512, "FCCU_MANUAL_PURGE", [0x01])
        );
        assert_eq!(
            encode_fccu_command("fan_on", false).unwrap(),
            (0x513, "FCCU_FAN", [0x00])
        );
        assert!(encode_fccu_command("invalid_cmd", true).is_err());
    }

    #[test]
    fn test_encode_fccu_settings() {
        let (fan_data, valve_data) = encode_fccu_settings(2, 300, 1000, 500, 255);
        assert_eq!(fan_data[0], 2);
        assert_eq!(fan_data[1], 0);
        assert_eq!(u16::from_le_bytes([fan_data[2], fan_data[3]]), 1000);
        assert_eq!(u16::from_le_bytes([fan_data[4], fan_data[5]]), 500);
        assert_eq!(u16::from_le_bytes([fan_data[6], fan_data[7]]), 255);

        assert_eq!(u16::from_le_bytes([valve_data[0], valve_data[1]]), 300);
        assert_eq!(u16::from_le_bytes([valve_data[2], valve_data[3]]), 100);

        // Fan mode masked to 2 bits
        let (masked_fan, _) = encode_fccu_settings(0xFF, 10, 0, 0, 0);
        assert_eq!(masked_fan[0], 0x03);
    }

    #[test]
    fn test_encode_lighting_bitmask() {
        assert_eq!(encode_lighting(false, false, false, false, false, false), 0x00);
        assert_eq!(encode_lighting(true, false, false, false, false, false), 0b0000_0001);
        assert_eq!(encode_lighting(false, true, false, true, false, true), 0b0010_1010);
        assert_eq!(encode_lighting(true, true, true, true, true, true), 0b0011_1111);
    }

    #[test]
    fn test_parse_pcan_channel() {
        assert_eq!(parse_pcan_channel("1"), Some(0x51));
        assert_eq!(parse_pcan_channel("2"), Some(0x52));
        assert_eq!(parse_pcan_channel("0x53"), Some(0x53));
        assert_eq!(parse_pcan_channel("0X58"), Some(0x58));
        assert_eq!(parse_pcan_channel("81"), Some(0x51));
        assert_eq!(parse_pcan_channel("0"), None);
        assert_eq!(parse_pcan_channel("invalid"), None);
    }

    #[test]
    fn test_validate_db_config() {
        let valid = DbConfig::default();
        assert!(validate_db_config(&valid).is_ok());

        let empty_host = DbConfig { host: "   ".into(), ..valid.clone() };
        assert!(validate_db_config(&empty_host).is_err());

        let zero_port = DbConfig { port: 0, ..valid.clone() };
        assert!(validate_db_config(&zero_port).is_err());

        let empty_db = DbConfig { dbname: "".into(), ..valid.clone() };
        assert!(validate_db_config(&empty_db).is_err());

        let empty_user = DbConfig { user: "".into(), ..valid };
        assert!(validate_db_config(&empty_user).is_err());
    }
}
