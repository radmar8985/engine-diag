mod pcan;

use pcan::{PcanDriver, PCAN_BAUD_500K};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio_postgres::NoTls;

// ── Data models ──

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

/// CAN data received from the bus — maps to DBC signals
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
struct CanReadData {
    // MCU_ANALOG_FUEL_CELL (0x104)
    fc_output_voltage: f64,
    fc_output_current: f64,
    h2_high_pressure: f64,
    // MCU_ANALOG_POWERTRAIN (0x105)
    sc_voltage: f64,
    sc_current: f64,
    mc_supply_voltage: f64,
    mc_supply_current: f64,
    // MCU_ANALOG_ACCESSORY (0x106)
    ab_voltage: f64,
    ab_current: f64,
    // MCU_ANALOG_DRIVE (0x107)
    rpm: f64,
    speed: f64,
    // FCCU_POWER (0x503)
    fccu_fc_voltage: f64,
    fccu_fc_current: f64,
    fccu_dcdc_voltage: f64,
    fccu_dcdc_current: f64,
    // FCCU_ENVIRONMENT (0x504)
    fc_temp: f64,
    ambient_temp: f64,
    ambient_humidity: f64,
    // FCCU_STATE (0x500)
    fccu_state: u8,
    fccu_main_valve: bool,
    fccu_purge_valve: bool,
    fccu_fan_state: bool,
    fccu_fan_duty: u8,
    // FCCU_VERSION (0x501)
    fccu_version: String,
    // FCCU_HYDROGEN (0x502)
    h2_flow: f64,
    h2_lp_pressure: f64,
    // PROTIUM_POWER (0x200)
    fc_w: f64,
    energy_wh: f64,
    // MCU_STATE (0x100)
    mcu_status: u8,
    vehicle_type: u8,
    motor_controller_enabled: bool,
    main_valve_enabled: bool,
}

// ── Shared PCAN state ──

struct CanState(Option<Arc<PcanDriver>>);

// ── PostgreSQL stream (Grafana tab) ──

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

        println!(
            "Łączenie z PostgreSQL: host={} port={} user={} dbname={}",
            pg_host, pg_port, pg_user, pg_dbname
        );

        let client = loop {
            match tokio_postgres::connect(&conn_string, NoTls).await {
                Ok((client, connection)) => {
                    tokio::spawn(async move {
                        if let Err(e) = connection.await {
                            eprintln!("PostgreSQL connection error: {:?}", e);
                        }
                    });
                    println!("Połączono z PostgreSQL/TimescaleDB!");
                    break client;
                }
                Err(e) => {
                    eprintln!(
                        "Błąd połączenia z PostgreSQL: {:?}. Ponowna próba za 3s...",
                        e
                    );
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

// ── CAN frame decoder ──

/// Decode a single CAN frame into the aggregated CanReadData struct.
/// IDs are already masked (29-bit, no 0x80000000 flag).
fn decode_can_frame(data: &mut CanReadData, id: u32, payload: &[u8], len: u8) {
    let n = len as usize;
    match id {
        // MCU_STATE (DBC ID 0x80000100 → 0x100)
        0x100 | 0x00 if n >= 2 => {
            data.mcu_status = payload[0] & 0x0F;
            data.vehicle_type = (payload[0] >> 4) & 0x0F;
            data.motor_controller_enabled = (payload[1] & 0x40) != 0; // bit 14
            data.main_valve_enabled = (payload[1] & 0x80) != 0; // bit 15
        }
        // MCU_ANALOG_FUEL_CELL (DBC ID 0x80000104 → 0x104)
        0x104 | 0x04 if n >= 6 => {
            // fc_output_current: start_bit=0, length=16, LE, signed, factor=0.001
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.fc_output_current = raw as f64 * 0.001;
            // fc_output_voltage: start_bit=16, length=16, LE, signed, factor=0.001
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.fc_output_voltage = raw as f64 * 0.001;
            // h2_high_pressure: start_bit=32, length=16, LE, unsigned, factor=0.1
            let raw = u16::from_le_bytes([payload[4], payload[5]]);
            data.h2_high_pressure = raw as f64 * 0.1;
        }
        // MCU_ANALOG_POWERTRAIN (DBC ID 0x80000105 → 0x105)
        0x105 | 0x05 if n >= 8 => {
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.sc_voltage = raw as f64 * 0.001;
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.sc_current = raw as f64 * 0.01;
            let raw = i16::from_le_bytes([payload[4], payload[5]]);
            data.mc_supply_current = raw as f64 * 0.001;
            let raw = i16::from_le_bytes([payload[6], payload[7]]);
            data.mc_supply_voltage = raw as f64 * 0.001;
        }
        // MCU_ANALOG_ACCESSORY (DBC ID 0x80000106 → 0x106)
        0x106 | 0x06 if n >= 4 => {
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.ab_current = raw as f64 * 0.001;
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.ab_voltage = raw as f64 * 0.001;
        }
        // MCU_ANALOG_DRIVE (DBC ID 0x80000107 → 0x107)
        0x107 | 0x07 if n >= 4 => {
            let raw = u16::from_le_bytes([payload[0], payload[1]]);
            data.rpm = raw as f64; // factor=1
            let raw = u16::from_le_bytes([payload[2], payload[3]]);
            data.speed = raw as f64 * 0.01;
        }
        // PROTIUM_POWER (DBC ID 0x80000200 → 0x200)
        0x200 if n >= 8 => {
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.fc_output_voltage = raw as f64 * 0.01; // FC_V
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.fc_output_current = raw as f64 * 0.01; // FC_A
            let raw = i16::from_le_bytes([payload[4], payload[5]]);
            data.fc_w = raw as f64; // FC_W, factor=1
            let raw = u16::from_le_bytes([payload[6], payload[7]]);
            data.energy_wh = raw as f64; // ENERGY_WH, factor=1
        }
        // FCCU_STATE (DBC ID 0x80000500 → 0x500)
        0x500 if n >= 2 => {
            data.fccu_state = payload[0] & 0x0F;
            data.fccu_main_valve = (payload[0] & 0x10) != 0;
            data.fccu_purge_valve = (payload[0] & 0x20) != 0;
            data.fccu_fan_state = (payload[0] & 0x40) != 0;
            data.fccu_fan_duty = payload[1];
        }
        // FCCU_VERSION (DBC ID 0x80000501 → 0x501)
        0x501 if n >= 3 => {
            data.fccu_version = format!("v{}.{}.{}", payload[0], payload[1], payload[2]);
        }
        // FCCU_HYDROGEN (DBC ID 0x80000502 → 0x502)
        0x502 if n >= 4 => {
            let raw = u16::from_le_bytes([payload[0], payload[1]]);
            data.h2_flow = raw as f64 * 0.01;
            let raw = u16::from_le_bytes([payload[2], payload[3]]);
            data.h2_lp_pressure = raw as f64 * 0.01;
        }
        // FCCU_POWER (DBC ID 0x80000503 → 0x503)
        0x503 | 0x103 if n >= 8 => {
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.fccu_fc_voltage = raw as f64 * 0.01;
            data.fc_output_voltage = data.fccu_fc_voltage;
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.fccu_fc_current = raw as f64 * 0.01;
            data.fc_output_current = data.fccu_fc_current;
            data.fc_w = data.fc_output_voltage * data.fc_output_current;
            let raw = i16::from_le_bytes([payload[4], payload[5]]);
            data.fccu_dcdc_voltage = raw as f64 * 0.01;
            let raw = i16::from_le_bytes([payload[6], payload[7]]);
            data.fccu_dcdc_current = raw as f64 * 0.01;
        }
        // FCCU_ENVIRONMENT (DBC ID 0x80000504 → 0x504)
        0x504 if n >= 6 => {
            let raw = i16::from_le_bytes([payload[0], payload[1]]);
            data.fc_temp = raw as f64 * 0.01;
            let raw = i16::from_le_bytes([payload[2], payload[3]]);
            data.ambient_temp = raw as f64 * 0.01;
            let raw = u16::from_le_bytes([payload[4], payload[5]]);
            data.ambient_humidity = raw as f64 * 0.01;
        }
        _ => {} // Unknown or short frame — ignore
    }
}

// ── CAN receive stream (CAN tab) ──

fn start_can_stream(app_handle: AppHandle, pcan: Option<Arc<PcanDriver>>) {
    tauri::async_runtime::spawn(async move {
        let driver = match pcan {
            Some(d) => d,
            None => {
                eprintln!("[CAN] Brak adaptera PCAN — stream CAN nie zostanie uruchomiony.");
                return;
            }
        };

        println!("[CAN] Stream CAN uruchomiony — odczyt z PCAN-USB");

        let mut accumulated = CanReadData::default();

        loop {
            // Drain all available frames from the receive buffer
            let mut frames_read = 0u32;
            loop {
                match driver.read() {
                    Ok(Some(msg)) => {
                        let can_id = msg.id & 0x1FFFFFFF; // mask to 29-bit
                        decode_can_frame(
                            &mut accumulated,
                            can_id,
                            &msg.data,
                            msg.len,
                        );
                        frames_read += 1;
                        // Safety limit to prevent blocking forever if bus is very busy
                        if frames_read >= 500 {
                            break;
                        }
                    }
                    Ok(None) => break, // buffer empty
                    Err(err) => {
                        eprintln!("[CAN] Read error: 0x{:05X}", err);
                        // Small delay before retry on error
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        break;
                    }
                }
            }

            // Emit accumulated data to frontend at ~20 Hz
            let _ = app_handle.emit("can-data-update", accumulated.clone());

            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
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
    let (can_id, name) = match command_type.as_str() {
        "system_enable" => (0x510u32, "FCCU_SYSTEM_ENABLE"),
        "main_valve" => (0x511, "FCCU_MAIN_VALVE_COMMAND"),
        "manual_purge" => (0x512, "FCCU_MANUAL_PURGE"),
        "fan_on" => (0x513, "FCCU_FAN"),
        _ => return Err(format!("Nieznana komenda FCCU: {}", command_type)),
    };

    let payload = [state as u8];
    println!(
        "[CAN TX] {} (0x{:08X}): [0x{:02X}] → {}",
        name,
        can_id,
        payload[0],
        if state { "ON" } else { "OFF" }
    );

    if let Some(ref driver) = can_state.0 {
        driver.write(can_id, &payload).map_err(|e| {
            format!("CAN write error 0x{:05X} for {}", e, name)
        })?;
    } else {
        println!("[CAN TX] (brak adaptera — ramka nie została wysłana)");
    }

    Ok(format!(
        "{} → {}",
        name,
        if state { "ON" } else { "OFF" }
    ))
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
    // 0x520: FCCU_FAN_SETTINGS (8 bajtów: fan_mode=bits 0-1, p_gain=bytes 2-3, i_gain=bytes 4-5, d_gain=bytes 6-7)
    let fan_data: [u8; 8] = [
        (fan_mode & 0x03) as u8,
        0x00,
        (p_gain & 0xFF) as u8,
        ((p_gain >> 8) & 0xFF) as u8,
        (i_gain & 0xFF) as u8,
        ((i_gain >> 8) & 0xFF) as u8,
        (d_gain & 0xFF) as u8,
        ((d_gain >> 8) & 0xFF) as u8,
    ];

    // 0x521: FCCU_VALVE_SETTINGS (4 bajty: purge_interval_s=bytes 0-1, purge_duration_ms=bytes 2-3)
    let valve_data: [u8; 4] = [
        (purge_interval & 0xFF) as u8,
        ((purge_interval >> 8) & 0xFF) as u8,
        100u8, // domyślny czas zrzutu 100 ms (zakres FCCU: 10-500 ms)
        0x00,
    ];

    println!(
        "[CAN TX] FCCU_FAN_SETTINGS (0x520): {:02X?}  (fan_mode={}, P={}, I={}, D={})",
        fan_data, fan_mode, p_gain, i_gain, d_gain
    );
    println!(
        "[CAN TX] FCCU_VALVE_SETTINGS (0x521): {:02X?}  (purge_interval={}s, duration=100ms)",
        valve_data, purge_interval
    );

    if let Some(ref driver) = can_state.0 {
        driver
            .write(0x520, &fan_data)
            .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_FAN_SETTINGS (0x520)", e))?;
        driver
            .write(0x521, &valve_data)
            .map_err(|e| format!("CAN write error 0x{:05X} for FCCU_VALVE_SETTINGS (0x521)", e))?;
    } else {
        println!("[CAN TX] (brak adaptera — ramki nie zostały wysłane)");
    }

    Ok(format!(
        "Wysłano FCCU Settings: fan_mode={}, purge={}s, PID=({},{},{})",
        fan_mode, purge_interval, p_gain, i_gain, d_gain
    ))
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
    let byte0: u8 = (headlight as u8)
        | ((position_light as u8) << 1)
        | ((brake_light as u8) << 2)
        | ((left_indicator as u8) << 3)
        | ((right_indicator as u8) << 4)
        | ((hazard as u8) << 5);

    println!(
        "[CAN TX] MCU_LIGHTING (0x400): [0x{:02X}]  H={} P={} B={} L={} R={} Hz={}",
        byte0, headlight, position_light, brake_light, left_indicator, right_indicator, hazard
    );

    if let Some(ref driver) = can_state.0 {
        driver
            .write(0x400, &[byte0])
            .map_err(|e| format!("CAN write error 0x{:05X} for MCU_LIGHTING", e))?;
    } else {
        println!("[CAN TX] (brak adaptera — ramka nie została wysłana)");
    }

    Ok(format!(
        "Wysłano Lighting: H={} P={} B={} L={} R={} Hz={}",
        headlight, position_light, brake_light, left_indicator, right_indicator, hazard
    ))
}

// ── App entry point ──

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    dotenvy::dotenv().ok();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            send_fccu_command,
            send_fccu_settings,
            send_lighting,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let can_handle = app.handle().clone();

            // Start PostgreSQL stream (Grafana tab)
            start_pg_stream(app_handle);

            // Try to initialize PCAN-USB adapter (auto-detect free channel or read PCAN_CHANNEL env var)
            let preferred_channel = std::env::var("PCAN_CHANNEL").ok().and_then(|v| {
                if v.starts_with("0x") || v.starts_with("0X") {
                    u16::from_str_radix(&v[2..], 16).ok()
                } else if let Ok(num) = v.parse::<u16>() {
                    if num < 0x50 {
                        Some(0x50 + num) // e.g. "1" -> 0x51, "2" -> 0x52
                    } else {
                        Some(num)
                    }
                } else {
                    None
                }
            });

            let pcan_arc = match PcanDriver::new_auto(preferred_channel, PCAN_BAUD_500K) {
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

            // Manage PCAN state for commands
            app.manage(CanState(pcan_arc.clone()));

            // Start CAN receive stream
            start_can_stream(can_handle, pcan_arc);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
