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
 * Live CAN bus data decoded from vehicle DBC signals
 */
export interface CanReadData {
  // MCU_ANALOG_FUEL_CELL (0x104)
  fc_output_voltage: number;
  fc_output_current: number;
  h2_high_pressure: number;
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
  // FCCU_POWER (0x503)
  fccu_fc_voltage: number;
  fccu_fc_current: number;
  fccu_dcdc_voltage: number;
  fccu_dcdc_current: number;
  // FCCU_ENVIRONMENT (0x504)
  fc_temp: number;
  ambient_temp: number;
  ambient_humidity: number;
  // FCCU_STATE (0x500)
  fccu_state: number;
  fccu_main_valve: boolean;
  fccu_purge_valve: boolean;
  fccu_fan_state: boolean;
  fccu_fan_duty: number;
  // FCCU_VERSION (0x501)
  fccu_version: string;
  // FCCU_HYDROGEN (0x502)
  h2_flow: number;
  h2_lp_pressure: number;
  // PROTIUM_POWER (0x200)
  fc_w: number;
  energy_wh: number;
  // MCU_STATE (0x100)
  mcu_status: number;
  vehicle_type: number;
  motor_controller_enabled: boolean;
  main_valve_enabled: boolean;
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
