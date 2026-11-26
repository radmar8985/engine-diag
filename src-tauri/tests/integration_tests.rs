use engine_diag_lib::{
    apply_db_config, create_dbc_parser, decode_can_frame, disconnect_db_state,
    execute_fccu_command, execute_fccu_settings, execute_lighting, format_pcan_channel_label,
    load_db_config_from_path, read_can_connection_info, read_can_status, reconnect_db_state,
    CanReadData, CanState, DbConfig, DbConfigState, DbStatus, EngineData,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

#[test]
fn integration_db_config_persistence_and_watch_channel() {
    let temp_env = std::env::temp_dir().join(format!(
        "engine_diag_test_{}.env",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&temp_env);

    let (config_tx, mut config_rx) = tokio::sync::watch::channel(DbConfig::default());
    let db_status = Arc::new(RwLock::new(DbStatus {
        connected: false,
        message: "Inicjalizacja...".to_string(),
    }));
    let db_state = DbConfigState {
        tx: config_tx,
        status: db_status.clone(),
        manual_disconnected: Arc::new(AtomicBool::new(false)),
    };

    let updated = DbConfig {
        host: "192.168.10.20".to_string(),
        port: 5433,
        user: "telemetry".to_string(),
        password: "secret_password".to_string(),
        dbname: "hydrive_db".to_string(),
    };

    let msg = apply_db_config(updated.clone(), &db_state, Some(&temp_env))
        .expect("apply_db_config should succeed for valid config");
    assert!(msg.contains("Dane zapisane pomyślnie"));

    // 1. Verify watch channel propagated the new config immediately to stream listener
    assert!(config_rx.has_changed().unwrap());
    let received = config_rx.borrow_and_update().clone();
    assert_eq!(received, updated);

    // 2. Verify .env file on disk persisted and reloads identically
    let reloaded = load_db_config_from_path(&temp_env);
    assert_eq!(reloaded, updated);

    // 3. Verify validation error leaves watch channel and disk untouched
    let invalid = DbConfig {
        host: "".to_string(),
        ..updated.clone()
    };
    assert!(apply_db_config(invalid, &db_state, Some(&temp_env)).is_err());
    assert!(!config_rx.has_changed().unwrap());

    // 4. Verify manual disconnect and reconnect flags & status
    let disc = disconnect_db_state(&db_state);
    assert!(!disc.connected);
    assert_eq!(disc.message, "Rozłączono ręcznie");
    assert!(db_state.manual_disconnected.load(Ordering::SeqCst));

    reconnect_db_state(&db_state);
    assert!(!db_state.manual_disconnected.load(Ordering::SeqCst));

    let _ = std::fs::remove_file(&temp_env);
}

#[test]
fn integration_can_state_and_commands_without_hardware() {
    let can_state = CanState::new(Arc::new(RwLock::new(None)), Arc::new(AtomicBool::new(false)));

    assert!(!read_can_status(&can_state));
    let info = read_can_connection_info(&can_state);
    assert!(!info.connected);
    assert_eq!(info.channel, None);
    assert_eq!(format_pcan_channel_label(0x51), "PCAN_USBBUS1 (0x51)");
    assert_eq!(format_pcan_channel_label(0x52), "PCAN_USBBUS2 (0x52)");

    let cmd_err = execute_fccu_command("main_valve", true, &can_state).unwrap_err();
    assert!(cmd_err.contains("Brak adaptera PCAN-USB"));

    let unknown_err = execute_fccu_command("non_existent", true, &can_state).unwrap_err();
    assert!(unknown_err.contains("Nieznana komenda FCCU"));

    let settings_err = execute_fccu_settings(2, 60, 100, 50, 10, &can_state).unwrap_err();
    assert!(settings_err.contains("Brak adaptera PCAN-USB"));

    let lighting_err =
        execute_lighting(true, false, true, false, false, false, &can_state).unwrap_err();
    assert!(lighting_err.contains("Brak adaptera PCAN-USB"));
}

#[test]
fn integration_dbc_multi_frame_stream_and_json_ipc_serialization() {
    let parser = create_dbc_parser();
    let mut accumulated = CanReadData::default();

    let frames: Vec<(u32, [u8; 8])> = vec![
        // MCU_ANALOG_POWERTRAIN (0x105 with extended ID flag)
        (0x8000_0105, {
            let mut b = [0u8; 8];
            b[0..2].copy_from_slice(&28000i16.to_le_bytes()); // 28.0 V SC
            b[2..4].copy_from_slice(&500i16.to_le_bytes());   // 5.0 A SC
            b[4..6].copy_from_slice(&10000i16.to_le_bytes()); // 10.0 A MC
            b[6..8].copy_from_slice(&31000i16.to_le_bytes()); // 31.0 V MC
            b
        }),
        // MCU_ANALOG_DRIVE (0x107)
        (0x107, {
            let mut b = [0u8; 8];
            b[0..2].copy_from_slice(&2800u16.to_le_bytes());
            b[2..4].copy_from_slice(&3500u16.to_le_bytes());
            b
        }),
        // FCCU_POWER (0x503)
        (0x503, {
            let mut b = [0u8; 8];
            b[0..2].copy_from_slice(&4600i16.to_le_bytes()); // 46.0 V
            b[2..4].copy_from_slice(&1200i16.to_le_bytes()); // 12.0 A -> 552 W
            b
        }),
        // FCCU_ENVIRONMENT (0x504)
        (0x504, {
            let mut b = [0u8; 8];
            b[0..2].copy_from_slice(&6150i16.to_le_bytes()); // 61.5 C
            b
        }),
    ];

    for (id, payload) in &frames {
        decode_can_frame(&parser, &mut accumulated, *id, payload);
    }

    // Verify JSON serialization matches the contract expected by Svelte frontend
    let json = serde_json::to_value(&accumulated).expect("CanReadData must serialize to JSON");
    assert_eq!(json["sc_voltage"], 28.0);
    assert_eq!(json["mc_supply_voltage"], 31.0);
    assert_eq!(json["rpm"], 2800.0);
    assert_eq!(json["fc_output_voltage"], 46.0);
    assert_eq!(json["fc_w"], 552.0);
    assert_eq!(json["fc_temp"], 61.5);

    let engine_sample = EngineData {
        voltage1: 48.0,
        rpm: 1500.0,
        ..Default::default()
    };
    let engine_json = serde_json::to_string(&engine_sample).unwrap();
    let roundtrip: EngineData = serde_json::from_str(&engine_json).unwrap();
    assert_eq!(roundtrip, engine_sample);
}
