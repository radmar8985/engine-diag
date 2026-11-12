use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
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
    // MCU_ANALOG_FUEL_CELL
    fc_output_voltage: f64,
    fc_output_current: f64,
    h2_high_pressure: f64,
    // MCU_ANALOG_POWERTRAIN
    sc_voltage: f64,
    sc_current: f64,
    mc_supply_voltage: f64,
    mc_supply_current: f64,
    // MCU_ANALOG_ACCESSORY
    ab_voltage: f64,
    ab_current: f64,
    // MCU_ANALOG_DRIVE
    rpm: f64,
    speed: f64,
    // FCCU_POWER
    fccu_fc_voltage: f64,
    fccu_fc_current: f64,
    fccu_dcdc_voltage: f64,
    fccu_dcdc_current: f64,
    // FCCU_ENVIRONMENT
    fc_temp: f64,
    ambient_temp: f64,
    ambient_humidity: f64,
    // PROTIUM_POWER
    fc_w: f64,
    energy_wh: f64,
    // MCU_STATE
    mcu_status: u8,
    vehicle_type: u8,
    motor_controller_enabled: bool,
    main_valve_enabled: bool,
}

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

        println!("Łączenie z PostgreSQL: host={} port={} user={} dbname={}", pg_host, pg_port, pg_user, pg_dbname);

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

// ── CAN receive stream (CAN tab) ──

fn start_can_stream(app_handle: AppHandle) {
    tauri::async_runtime::spawn(async move {
        // TODO: Replace with actual CAN interface initialization
        // For Linux: use socketcan crate with "can0" interface
        // For Windows: use your USB-CAN adapter's library (PCAN, Kvaser, etc.)
        //
        // Example with socketcan (Linux only):
        //   let socket = CANSocket::open("can0").expect("Failed to open CAN interface");
        //   loop {
        //       let frame = socket.read_frame().unwrap();
        //       match frame.id() {
        //           0x80000004 => { /* MCU_ANALOG_FUEL_CELL */ }
        //           0x80000005 => { /* MCU_ANALOG_POWERTRAIN */ }
        //           ...
        //       }
        //   }

        println!("[CAN] Stream CAN nie jest jeszcze podłączony do sprzętu.");
        println!("[CAN] Emitowanie danych testowych co 500ms...");

        let mut tick: f64 = 0.0;

        loop {
            // Simulated CAN data for development/testing
            let data = CanReadData {
                fc_output_voltage: 48.0 + (tick * 0.1).sin() * 2.0,
                fc_output_current: 12.0 + (tick * 0.15).sin() * 3.0,
                h2_high_pressure: 350.0 - tick * 0.5 % 50.0,
                sc_voltage: 24.0 + (tick * 0.08).sin() * 1.5,
                sc_current: 5.0 + (tick * 0.12).sin() * 2.0,
                mc_supply_voltage: 48.0 + (tick * 0.07).cos() * 1.0,
                mc_supply_current: 8.0 + (tick * 0.1).cos() * 2.0,
                ab_voltage: 12.5 + (tick * 0.05).sin() * 0.5,
                ab_current: 2.0 + (tick * 0.09).sin() * 1.0,
                rpm: 3500.0 + (tick * 0.2).sin() * 500.0,
                speed: 25.0 + (tick * 0.2).sin() * 5.0,
                fccu_fc_voltage: 48.0 + (tick * 0.1).sin() * 2.0,
                fccu_fc_current: 12.0 + (tick * 0.15).sin() * 3.0,
                fccu_dcdc_voltage: 24.0 + (tick * 0.08).cos() * 1.0,
                fccu_dcdc_current: 4.0 + (tick * 0.12).cos() * 1.5,
                fc_temp: 65.0 + (tick * 0.03).sin() * 5.0,
                ambient_temp: 22.0 + (tick * 0.01).sin() * 2.0,
                ambient_humidity: 45.0 + (tick * 0.02).sin() * 10.0,
                fc_w: 580.0 + (tick * 0.1).sin() * 100.0,
                energy_wh: tick * 0.16,
                mcu_status: 4,  // RUNNING
                vehicle_type: 0, // HYDROS_V1
                motor_controller_enabled: true,
                main_valve_enabled: true,
            };

            let _ = app_handle.emit("can-data-update", data);
            tick += 0.5;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
}

// ── CAN send commands ──

/// Send FCCU commands (single-bit toggles)
/// DBC messages: FCCU_SYSTEM_ENABLE, FCCU_MAIN_VALVE_COMMAND, FCCU_MANUAL_PURGE, FCCU_FAN
#[tauri::command]
fn send_fccu_commands(
    system_enable: bool,
    main_valve: bool,
    manual_purge: bool,
    fan_on: bool,
) -> Result<String, String> {
    // Build CAN frames per DBC:
    // FCCU_SYSTEM_ENABLE  (0x80000210): byte[0] bit 0 = CMD_SYSTEM_ENABLE
    // FCCU_MAIN_VALVE_CMD (0x80000211): byte[0] bit 0 = CMD_MAIN_VALVE
    // FCCU_MANUAL_PURGE   (0x80000212): byte[0] bit 0 = CMD_MANUAL_PURGE
    // FCCU_FAN            (0x80000213): byte[0] bit 0 = CMD_FAN_ON

    let frames = [
        (0x80000210u32, "FCCU_SYSTEM_ENABLE", system_enable as u8),
        (0x80000211, "FCCU_MAIN_VALVE_COMMAND", main_valve as u8),
        (0x80000212, "FCCU_MANUAL_PURGE", manual_purge as u8),
        (0x80000213, "FCCU_FAN", fan_on as u8),
    ];

    for (id, name, value) in &frames {
        println!("[CAN TX] {} (0x{:08X}): [0x{:02X}]", name, id, value);
        // TODO: Send actual CAN frame
        // socket.write_frame(&CANFrame::new(*id, &[*value], true, false).unwrap()).unwrap();
    }

    Ok(format!(
        "Wysłano komendy FCCU: sys={}, valve={}, purge={}, fan={}",
        system_enable, main_valve, manual_purge, fan_on
    ))
}

/// Send FCCU settings
/// DBC message: FCCU_SETTINGS (0x80000220)
#[tauri::command]
fn send_fccu_settings(
    fan_mode: u8,
    purge_interval: u16,
    p_gain: u16,
    i_gain: u16,
    d_gain: u16,
) -> Result<String, String> {
    // FCCU_SETTINGS (0x80000220), 8 bytes:
    //   FAN_MODE:          bits 0-1  (2 bits)
    //   PURGE_INTERVAL_S:  bits 2-15 (14 bits)
    //   P_GAIN:            bits 16-31
    //   I_GAIN:            bits 32-47
    //   D_GAIN:            bits 48-63

    let byte0_1: u16 = (fan_mode as u16 & 0x03) | ((purge_interval & 0x3FFF) << 2);
    let data: [u8; 8] = [
        (byte0_1 & 0xFF) as u8,
        ((byte0_1 >> 8) & 0xFF) as u8,
        (p_gain & 0xFF) as u8,
        ((p_gain >> 8) & 0xFF) as u8,
        (i_gain & 0xFF) as u8,
        ((i_gain >> 8) & 0xFF) as u8,
        (d_gain & 0xFF) as u8,
        ((d_gain >> 8) & 0xFF) as u8,
    ];

    println!(
        "[CAN TX] FCCU_SETTINGS (0x80000220): {:02X?}  (fan_mode={}, purge={}s, P={}, I={}, D={})",
        data, fan_mode, purge_interval, p_gain, i_gain, d_gain
    );

    // TODO: Send actual CAN frame
    // socket.write_frame(&CANFrame::new(0x80000220, &data, true, false).unwrap()).unwrap();

    Ok(format!(
        "Wysłano FCCU Settings: fan_mode={}, purge={}s, PID=({},{},{})",
        fan_mode, purge_interval, p_gain, i_gain, d_gain
    ))
}

/// Send MCU lighting commands
/// DBC message: MCU_LIGHTING (0x80000100)
#[tauri::command]
fn send_lighting(
    headlight: bool,
    position_light: bool,
    brake_light: bool,
    left_indicator: bool,
    right_indicator: bool,
    hazard: bool,
) -> Result<String, String> {
    // MCU_LIGHTING (0x80000100), 1 byte:
    //   HEADLIGHT:       bit 0
    //   POSITION_LIGHT:  bit 1
    //   BRAKE_LIGHT:     bit 2
    //   LEFT_INDICATOR:  bit 3
    //   RIGHT_INDICATOR: bit 4
    //   HAZARD:          bit 5

    let byte0: u8 = (headlight as u8)
        | ((position_light as u8) << 1)
        | ((brake_light as u8) << 2)
        | ((left_indicator as u8) << 3)
        | ((right_indicator as u8) << 4)
        | ((hazard as u8) << 5);

    println!(
        "[CAN TX] MCU_LIGHTING (0x80000100): [0x{:02X}]  H={} P={} B={} L={} R={} Hz={}",
        byte0, headlight, position_light, brake_light, left_indicator, right_indicator, hazard
    );

    // TODO: Send actual CAN frame
    // socket.write_frame(&CANFrame::new(0x80000100, &[byte0], true, false).unwrap()).unwrap();

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
            send_fccu_commands,
            send_fccu_settings,
            send_lighting,
        ])
        .setup(|app| {
            let app_handle = app.handle().clone();
            let can_handle = app.handle().clone();

            start_pg_stream(app_handle);
            start_can_stream(can_handle);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
