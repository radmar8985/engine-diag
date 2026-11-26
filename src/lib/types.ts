/**
 * Supported data sources for the dashboard
 */
export type DataSource = 'Grafana' | 'CAN';

/**
 * Engine sensor data streamed from PostgreSQL / TimescaleDB for the Grafana dashboard
 */
export interface EngineData {
  voltage1: number;
  voltage2: number;
  voltage3: number;
  current1: number;
  current2: number;
  current3: number;
  power1: number;
  power2: number;
  power3: number;
  temperature: number;
  rpm: number;
}

/**
 * Single timestamped flowmeter sample decoded from FCCU_HYDROGEN (0x502)
 */
export interface FlowSample {
  t_ms: number;
  flow: number;
}

/**
 * Live CAN bus data decoded from vehicle DBC signals
 */
export interface CanReadData {
  // MCU_ANALOG_FUEL_CELL (0x104)
  fc_output_voltage: number;
  fc_output_current: number;
  h2_high_pressure: number;
  h2_leakage_sensor_voltage?: number;
  // MCU_ANALOG_POWERTRAIN (0x105)
  sc_voltage: number;
  sc_current: number;
  mc_supply_voltage: number;
  mc_supply_current: number;
  // MCU_ANALOG_ACCESSORY (0x106)
  ab_voltage: number;
  ab_current: number;
  // MCU_ANALOG_DRIVE (0x107)
  rpm: number;
  speed: number;
  // MCU_ANALOG_PEDALS (0x103) & MCU_ANALOG_OUTPUTS (0x109)
  accel_pedal_voltage?: number;
  brake_pedal_voltage?: number;
  accel_output_voltage?: number;
  brake_output_voltage?: number;
  // FCCU_POWER (0x503)
  fccu_fc_voltage: number;
  fccu_fc_current: number;
  fccu_dcdc_voltage: number;
  fccu_dcdc_current: number;
  // FCCU_ENVIRONMENT (0x504)
  fc_temp: number;
  ambient_temp: number;
  ambient_humidity: number;
  ambient_pressure?: number;
  // FCCU_STATE (0x500)
  fccu_state: number;
  fccu_main_valve: boolean;
  fccu_purge_valve: boolean;
  fccu_fan_state: boolean;
  fccu_fan_duty: number;
  // FCCU_VERSION (0x501)
  fccu_version: string;
  fccu_build_id?: number;
  fccu_vehicle_type?: number;
  fccu_fuelcell_type?: number;
  // FCCU_HYDROGEN (0x502) & FCCU_HYDROGEN_TOTAL (0x506)
  h2_flow: number;
  h2_flow_samples?: FlowSample[];
  h2_lp_pressure: number;
  h2_hp_pressure?: number;
  fccu_leakage_voltage?: number;
  h2_total_volume?: number;
  // FCCU_CURRENTS (0x505)
  fccu_load_current?: number;
  fccu_ho_current_1?: number;
  fccu_ho_current_2?: number;
  fccu_ho_current_3?: number;
  // PROTIUM_POWER (0x200)
  fc_w: number;
  energy_wh: number;
  // MCU_STATE (0x100) & MCU_FAULTS (0x101)
  mcu_status: number;
  vehicle_type: number;
  vehicle_allowed_to_move?: boolean;
  motor_controller_enabled: boolean;
  main_valve_enabled: boolean;
  mcu_active_faults?: string[];
  // CCU_STATUS (0x10F)
  ccu_status?: number;
  // SWU_STATUS (0x300)
  swu_can_state?: number;
  lap_number?: number;
  lap_time_ms?: number;
  total_time_ms?: number;
  // LCU_STATUS (0x401)
  lcu_front_fault?: boolean;
  lcu_back_fault?: boolean;
}

/**
 * Sidebar live metric readings state
 */
export type CurrentReadings = EngineData;

/**
 * Metric threshold boundary range
 */
export interface ThresholdRange {
  min: number;
  max: number;
}

/**
 * Status of a metric card based on threshold limits
 */
export type MetricStatus = 'normal' | 'warning' | 'alarm';

/**
 * Threshold configuration across all monitored sensor channels
 */
export interface MetricThresholds {
  voltage: ThresholdRange;
  current: ThresholdRange;
  temperature: ThresholdRange;
  rpm: ThresholdRange;
  power: ThresholdRange;
}

/**
 * FCCU operational state enum
 */
export enum FccuState {
  Init = 0,
  Starting = 1,
  Running = 2,
  Stopped = 3,
}

/**
 * FCCU fan mode enum
 */
export enum FccuFanMode {
  Manual = 0,
  LinearTemp = 1,
  PidController = 2,
}

/**
 * Available FCCU command action types
 */
export type FccuCommandType =
  | 'system_enable'
  | 'main_valve'
  | 'manual_purge'
  | 'fan_on';

/**
 * FCCU command toggles state
 */
export interface FccuCommandsState {
  systemEnable: boolean;
  mainValve: boolean;
  manualPurge: boolean;
  fanOn: boolean;
}

/**
 * FCCU parameters and PID settings
 */
export interface FccuSettings {
  fanMode: FccuFanMode | number;
  purgeInterval: number;
  pGain: number;
  iGain: number;
  dGain: number;
}

/**
 * Vehicle exterior lighting states
 */
export interface LightingState {
  headlight: boolean;
  positionLight: boolean;
  brakeLight: boolean;
  leftIndicator: boolean;
  rightIndicator: boolean;
  hazard: boolean;
}

/**
 * Statistical readings for a single chart metric
 */
export interface ChartSeriesStats {
  newest: number;
  average: number;
  rms: number;
}

/**
 * Statistical summary for CAN live charts
 */
export interface CanStatistics {
  chart1: ChartSeriesStats;
  chart2: ChartSeriesStats;
  chart3: ChartSeriesStats;
}

/**
 * PostgreSQL / TimescaleDB connection configuration
 */
export interface DbConfig {
  host: string;
  port: number;
  user: string;
  password: string;
  dbname: string;
}

/**
 * PostgreSQL connection status
 */
export interface DbStatus {
  connected: boolean;
  message: string;
}

/**
 * PCAN-USB connection status and active channel details
 */
export interface CanConnectionInfo {
  connected: boolean;
  channel: number | null;
  channel_label: string;
  baudrate_label: string;
  manual_disconnected: boolean;
}
