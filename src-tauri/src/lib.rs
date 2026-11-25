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

pub type PcanHolder = Arc<RwLock<Option<Arc<PcanDriver>>>>;

pub struct CanState(pub PcanHolder);

impl CanState {
    fn driver(&self) -> Option<Arc<PcanDriver>> {
        self.0.read().ok().and_then(|guard| guard.clone())
    }
}

fn set_driver(holder: &PcanHolder, driver: Option<Arc<PcanDriver>>) {
    if let Ok(mut guard) = holder.write() {
        *guard = driver;
    }
}

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
    let defaults = DbConfig::default();
    let env_or = |key: &str, default: String| std::env::var(key).unwrap_or(default);
    DbConfig {
        host: env_or("PG_HOST", defaults.host),
        port: std::env::var("PG_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(defaults.port),
        user: env_or("PG_USER", defaults.user),
        password: env_or("PG_PASSWORD", defaults.password),
        dbname: env_or("PG_DBNAME", defaults.dbname),
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

    let mut written = false;
    for p in [".env", "../.env"].map(std::path::Path::new) {
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

fn update_db_status(
    app_handle: &AppHandle,
    status_arc: &Arc<RwLock<DbStatus>>,
    connected: bool,
    message: String,
) {
    let status = DbStatus { connected, message };
    if let Ok(mut s) = status_arc.write() {
        *s = status.clone();
    }
    let _ = app_handle.emit("db-connection-status", status);
}

fn format_pg_error(e: &tokio_postgres::Error) -> String {
    if let Some(db_err) = e.as_db_error() {
        return match db_err.detail() {
            Some(detail) => format!("{} ({})", db_err.message(), detail),
            None => db_err.message().to_string(),
        };
    }
    if let Some(source) = std::error::Error::source(e) {
        return format!("{}: {}", e, source);
    }
    e.to_string()
}

fn build_pg_config(cfg: &DbConfig) -> tokio_postgres::Config {
    let mut pg_cfg = tokio_postgres::Config::new();
    pg_cfg
        .host(cfg.host.trim())
        .port(cfg.port)
        .user(cfg.user.trim())
        .password(&cfg.password)
        .dbname(cfg.dbname.trim());
    pg_cfg
}

fn start_pg_stream(
    app_handle: AppHandle,
    mut config_rx: tokio::sync::watch::Receiver<DbConfig>,
    status_arc: Arc<RwLock<DbStatus>>,
) {
    tauri::async_runtime::spawn(async move {
        loop {
            let current_config = config_rx.borrow_and_update().clone();
            let pg_config = build_pg_config(&current_config);

            update_db_status(
                &app_handle,
                &status_arc,
                false,
                format!("Łączenie z {}:{}...", current_config.host, current_config.port),
            );

            println!(
                "Łączenie z PostgreSQL: host={} port={} user={} dbname={}",
                current_config.host, current_config.port, current_config.user, current_config.dbname
            );

            let connect_result = tokio::select! {
                res = pg_config.connect(NoTls) => Some(res),
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
                    update_db_status(
                        &app_handle,
                        &status_arc,
                        true,
                        format!("Połączono ({})", current_config.dbname),
                    );
                    client
                }
                Some(Err(e)) => {
                    let details = format_pg_error(&e);
                    eprintln!("Błąd połączenia z PostgreSQL: {}. Ponowna próba za 3s...", details);
                    update_db_status(
                        &app_handle,
                        &status_arc,
                        false,
                        format!("Błąd połączenia: {}", details),
                    );

                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
                        _ = config_rx.changed() => {}
                    }
                    continue;
                }
                None => continue,
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
                                let get_f64 = |col: &str| row.get::<_, Option<f64>>(col).unwrap_or(0.0);
                                let data = EngineData {
                                    voltage1: get_f64("voltage1"),
                                    voltage2: get_f64("voltage2"),
                                    voltage3: get_f64("voltage3"),
                                    current1: get_f64("current1"),
                                    current2: get_f64("current2"),
                                    current3: get_f64("current3"),
                                    power1: get_f64("power1"),
                                    power2: get_f64("power2"),
                                    power3: get_f64("power3"),
                                    temperature: get_f64("temperature"),
                                    rpm: get_f64("rpm"),
                                };

                                let _ = app_handle.emit("engine-data-update", data);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let details = format_pg_error(&e);
                                eprintln!("PostgreSQL query error: {}", details);
                                update_db_status(
                                    &app_handle,
                                    &status_arc,
                                    false,
                                    format!("Błąd zapytania: {}", details),
                                );
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
    } else {
        ["can/vehicle.dbc", "src-tauri/can/vehicle.dbc"]
            .map(std::path::Path::new)
            .into_iter()
            .find(|p| p.exists())
            .is_some_and(|p| parser.add_from_dbc_file(p).is_ok())
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
    let Some(msg) = parser
        .decode_msg(id, &buf)
        .or_else(|| parser.decode_msg(id | (1 << 31), &buf))
    else {
        return;
    };

    macro_rules! set_phys {
        ($sig:expr, $target:expr) => {
            if let Some(s) = msg.signals.get($sig) {
                $target = s.value.physical;
            }
        };
    }
    macro_rules! set_bool {
        ($sig:expr, $target:expr) => {
            if let Some(s) = msg.signals.get($sig) {
                $target = s.value.physical > 0.0;
            }
        };
    }
    macro_rules! set_u8 {
        ($sig:expr, $target:expr) => {
            if let Some(s) = msg.signals.get($sig) {
                $target = s.value.raw.unwrap_or(s.value.physical as i128) as u8;
            }
        };
    }

    match msg.name.as_str() {
        "MCU_STATE" => {
            set_u8!("STATUS", data.mcu_status);
            set_u8!("VEHICLE_TYPE", data.vehicle_type);
            set_bool!("MOTOR_CONTROLLER_ENABLED", data.motor_controller_enabled);
            set_bool!("MAIN_VALVE_ENABLED", data.main_valve_enabled);
        }
        "MCU_ANALOG_FUEL_CELL" => {
            set_phys!("FUEL_CELL_OUTPUT_CURRENT", data.fc_output_current);
            set_phys!("FUEL_CELL_OUTPUT_VOLTAGE", data.fc_output_voltage);
            set_phys!("HYDROGEN_HIGH_PRESSURE", data.h2_high_pressure);
        }
        "MCU_ANALOG_POWERTRAIN" => {
            set_phys!("SUPERCAPACITOR_VOLTAGE", data.sc_voltage);
            set_phys!("SUPERCAPACITOR_CURRENT", data.sc_current);
            set_phys!("MOTOR_CONTROLLER_SUPPLY_CURRENT", data.mc_supply_current);
            set_phys!("MOTOR_CONTROLLER_SUPPLY_VOLTAGE", data.mc_supply_voltage);
        }
        "MCU_ANALOG_ACCESSORY" => {
            set_phys!("ACCESSORY_BATTERY_CURRENT", data.ab_current);
            set_phys!("ACCESSORY_BATTERY_VOLTAGE", data.ab_voltage);
        }
        "MCU_ANALOG_DRIVE" => {
            set_phys!("RPM", data.rpm);
            set_phys!("SPEED", data.speed);
        }
        "PROTIUM_POWER" => {
            set_phys!("FC_V", data.fc_output_voltage);
            set_phys!("FC_A", data.fc_output_current);
            set_phys!("FC_W", data.fc_w);
            set_phys!("ENERGY", data.energy_wh);
        }
        "FCCU_STATE" => {
            set_u8!("STATE", data.fccu_state);
            set_bool!("MAIN_VALVE", data.fccu_main_valve);
            set_bool!("PURGE_VALVE", data.fccu_purge_valve);
            set_bool!("FAN_STATE", data.fccu_fan_state);
            set_u8!("FAN_DUTY", data.fccu_fan_duty);
        }
        "FCCU_VERSION" => {
            let ver = |k: &str| msg.signals.get(k).map_or(0, |s| s.value.physical as u8);
            data.fccu_version = format!(
                "v{}.{}.{}",
                ver("FIRMWARE_MAJOR"),
                ver("FIRMWARE_MINOR"),
                ver("FIRMWARE_PATCH")
            );
        }
        "FCCU_HYDROGEN" => {
            set_phys!("H2_FLOW", data.h2_flow);
            set_phys!("H2_LP_PRESSURE", data.h2_lp_pressure);
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
            set_phys!("DCDC_VOLTAGE", data.fccu_dcdc_voltage);
            set_phys!("DCDC_CURRENT", data.fccu_dcdc_current);
        }
        "FCCU_ENVIRONMENT" => {
            set_phys!("FC_TEMP_C", data.fc_temp);
            set_phys!("AMBIENT_TEMP_C", data.ambient_temp);
            set_phys!("AMBIENT_HUMIDITY", data.ambient_humidity);
        }
        _ => {}
    }
}

// ── CAN receive stream (CAN tab) ──

/// Parses PCAN channel specification (e.g. "1" -> 0x51, "0x52" -> 0x52).
pub fn parse_pcan_channel(v: &str) -> Option<u16> {
    let trimmed = v.trim();
    if let Some(hex) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
        return u16::from_str_radix(hex, 16).ok();
    }
    match trimmed.parse::<u16>().ok()? {
        0 => None,
        num if num < 0x50 => Some(0x50 + num), // e.g. "1" -> 0x51, "2" -> 0x52
        num => Some(num),
    }
}

fn get_preferred_pcan_channel() -> Option<u16> {
    std::env::var("PCAN_CHANNEL")
        .ok()
        .and_then(|v| parse_pcan_channel(&v))
}

fn set_can_connected(app_handle: &AppHandle, was_connected: &mut bool, connected: bool) {
    if *was_connected != connected {
        let _ = app_handle.emit("can-connection-status", connected);
        *was_connected = connected;
    }
}

fn disconnect_can(
    app_handle: &AppHandle,
    pcan_holder: &PcanHolder,
    was_connected: &mut bool,
    accumulated: &mut CanReadData,
) {
    set_driver(pcan_holder, None);
    set_can_connected(app_handle, was_connected, false);
    *accumulated = CanReadData::default();
}

fn start_can_stream(app_handle: AppHandle, pcan_holder: PcanHolder) {
    tauri::async_runtime::spawn(async move {
        println!("[CAN] Pętla obsługi magistrali CAN uruchomiona (ciągłe nasłuchiwanie / auto-reconnect)");

        let parser = create_dbc_parser();
        let mut accumulated = CanReadData::default();
        let mut was_connected = false;

        loop {
            let driver_opt = pcan_holder.read().ok().and_then(|guard| guard.clone());

            let driver = match driver_opt {
                Some(d) => {
                    if !d.is_hardware_ok() {
                        eprintln!("[CAN] Wykryto fizyczne odłączenie adaptera PCAN-USB (sprawdzenie sprzętowe)");
                        disconnect_can(&app_handle, &pcan_holder, &mut was_connected, &mut accumulated);
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                    d
                }
                None => {
                    // Brak adaptera — ciągłe nasłuchiwanie USB w tle (auto-reconnect)
                    match PcanDriver::new_auto(get_preferred_pcan_channel(), PCAN_BAUD_500K) {
                        Ok(new_driver) => {
                            println!(
                                "[CAN] Automatycznie wykryto i połączono adapter PCAN-USB (kanał 0x{:02X})!",
                                new_driver.channel()
                            );
                            let arc_driver = Arc::new(new_driver);
                            set_driver(&pcan_holder, Some(arc_driver.clone()));
                            let _ = app_handle.emit("can-connection-status", true);
                            was_connected = true;
                            arc_driver
                        }
                        Err(_) => {
                            set_can_connected(&app_handle, &mut was_connected, false);
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            continue;
                        }
                    }
                }
            };

            set_can_connected(&app_handle, &mut was_connected, true);

            let mut frames_read = 0u32;
            let mut hardware_lost = false;
            loop {
                match driver.read() {
                    Ok(Some(msg)) => {
                        let can_id = msg.id & 0x1FFFFFFF; // mask to 29-bit
                        let len = (msg.len as usize).min(msg.data.len());
                        decode_can_frame(&parser, &mut accumulated, can_id, &msg.data[..len]);
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
                disconnect_can(&app_handle, &pcan_holder, &mut was_connected, &mut accumulated);
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

/// Encodes `FCCU_FAN (0x513)` 2-byte frame (`CMD_FAN_ON` at bit 0, `CMD_FAN_DUTY` 0..=100% at byte 1).
pub fn encode_fccu_fan(state: bool, duty: u8) -> (u32, &'static str, [u8; 2]) {
    (0x513, "FCCU_FAN", [state as u8, duty.min(100)])
}

/// Encodes FCCU fan & purge valve settings into `(FCCU_FAN_SETTINGS 0x520, FCCU_VALVE_SETTINGS 0x521)` payloads.
pub fn encode_fccu_settings(
    fan_mode: u8,
    purge_interval: u16,
    p_gain: u16,
    i_gain: u16,
    d_gain: u16,
) -> ([u8; 8], [u8; 4]) {
    let [p0, p1] = p_gain.to_le_bytes();
    let [i0, i1] = i_gain.to_le_bytes();
    let [d0, d1] = d_gain.to_le_bytes();
    let [pi0, pi1] = purge_interval.to_le_bytes();

    (
        [fan_mode & 0x03, 0x00, p0, p1, i0, i1, d0, d1],
        [pi0, pi1, 100u8, 0x00],
    )
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

/// Executes `FCCU_FAN (0x513)` transmission with explicit `CMD_FAN_ON` and `CMD_FAN_DUTY` (`0..=100%`).
pub fn execute_fccu_fan(state: bool, duty: u8, can_state: &CanState) -> Result<String, String> {
    let (can_id, name, payload) = encode_fccu_fan(state, duty);
    let clamped_duty = payload[1];
    let state_label = if state { "ON" } else { "OFF" };
    println!(
        "[CAN TX] {} (0x{:08X}): [0x{:02X}, 0x{:02X}] → {} ({}%)",
        name, can_id, payload[0], clamped_duty, state_label, clamped_duty
    );

    let driver = can_state
        .driver()
        .ok_or_else(|| "Brak adaptera PCAN-USB — ramka nie została wysłana".to_string())?;
    driver
        .write(can_id, &payload)
        .map_err(|e| format!("CAN write error 0x{:05X} for {}", e, name))?;

    Ok(format!("{} → {} ({}%)", name, state_label, clamped_duty))
}

/// Executes FCCU command transmission against shared `CanState`.
pub fn execute_fccu_command(
    command_type: &str,
    state: bool,
    can_state: &CanState,
) -> Result<String, String> {
    if command_type == "fan_on" {
        return execute_fccu_fan(state, if state { 100 } else { 0 }, can_state);
    }

    let (can_id, name, payload) = encode_fccu_command(command_type, state)?;
    let state_label = if state { "ON" } else { "OFF" };
    println!(
        "[CAN TX] {} (0x{:08X}): [0x{:02X}] → {}",
        name, can_id, payload[0], state_label
    );

    let driver = can_state
        .driver()
        .ok_or_else(|| "Brak adaptera PCAN-USB — ramka nie została wysłana".to_string())?;
    driver
        .write(can_id, &payload)
        .map_err(|e| format!("CAN write error 0x{:05X} for {}", e, name))?;

    Ok(format!("{} → {}", name, state_label))
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

    let driver = can_state
        .driver()
        .ok_or_else(|| "Brak adaptera PCAN-USB — ustawienia nie zostały wysłane".to_string())?;
    driver
        .write(0x520, &fan_data)
        .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_FAN_SETTINGS (0x520)", e))?;
    driver
        .write(0x521, &valve_data)
        .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_VALVE_SETTINGS (0x521)", e))?;

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

    let driver = can_state
        .driver()
        .ok_or_else(|| "Brak adaptera PCAN-USB — oświetlenie nie zostało wysłane".to_string())?;
    driver
        .write(0x400, &[byte0])
        .map_err(|e| format!("CAN write error 0x{:05X} for MCU_LIGHTING", e))?;

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
        .map(|guard| guard.as_ref().is_some_and(|driver| driver.is_hardware_ok()))
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

/// Send `FCCU_FAN (0x80000513 → 0x513)` with `CMD_FAN_ON` and `CMD_FAN_DUTY` (`0..=100%`).
#[tauri::command]
fn send_fccu_fan(
    state: bool,
    duty: u8,
    can_state: State<'_, CanState>,
) -> Result<String, String> {
    execute_fccu_fan(state, duty, &can_state)
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
    match PcanDriver::new_auto(get_preferred_pcan_channel(), PCAN_BAUD_500K) {
        Ok(driver) => {
            let ch = driver.channel();
            set_driver(&can_state.0, Some(Arc::new(driver)));
            let _ = app_handle.emit("can-connection-status", true);
            Ok(format!("Połączono z adapterem PCAN-USB (kanał 0x{:02X})", ch))
        }
        Err(e) => {
            set_driver(&can_state.0, None);
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
        .unwrap_or_else(|_| DbStatus {
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
        send_fccu_fan,
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
    let (config_tx, config_rx) = tokio::sync::watch::channel(load_initial_db_config());
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
            // Start PostgreSQL stream (Grafana tab)
            start_pg_stream(app.handle().clone(), config_rx, db_status);

            // Try to initialize PCAN-USB adapter (auto-detect free channel or read PCAN_CHANNEL env var)
            let pcan_initial = match PcanDriver::new_auto(get_preferred_pcan_channel(), PCAN_BAUD_500K) {
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
            start_can_stream(app.handle().clone(), pcan_holder);

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

        // 2-byte FCCU_FAN with CMD_FAN_ON and CMD_FAN_DUTY (0..=100%)
        let (fan_id, fan_name, fan_payload) = encode_fccu_fan(true, 75);
        assert_eq!((fan_id, fan_name, fan_payload), (0x513, "FCCU_FAN", [0x01, 75]));

        let (_, _, clamped_payload) = encode_fccu_fan(true, 150);
        assert_eq!(clamped_payload, [0x01, 100]);

        let parser = create_dbc_parser();
        let mut buf = [0u8; 8];
        buf[..2].copy_from_slice(&fan_payload);
        let decoded = parser.decode_msg(fan_id | (1 << 31), &buf).expect("FCCU_FAN in DBC");
        assert_eq!(decoded.signals["CMD_FAN_ON"].value.physical, 1.0);
        assert_eq!(decoded.signals["CMD_FAN_DUTY"].value.physical, 75.0);
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
