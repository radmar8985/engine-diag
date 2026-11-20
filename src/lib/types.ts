export interface EngineData {
  voltage1: number;
  voltage2: number;
  voltage3: number;
  current1: number;
  current2: number;
  current3: number;
  temperature: number;
  rpm: number;
  power1: number;
  power2: number;
  power3: number;
}

export interface CanReadData {
  fc_output_voltage: number;
  fc_output_current: number;
  h2_high_pressure: number;
  sc_voltage: number;
  sc_current: number;
  mc_supply_voltage: number;
  mc_supply_current: number;
  ab_voltage: number;
  ab_current: number;
  rpm: number;
  speed: number;
  fccu_fc_voltage: number;
  fccu_fc_current: number;
  fccu_dcdc_voltage: number;
  fccu_dcdc_current: number;
  fc_temp: number;
  ambient_temp: number;
  ambient_humidity: number;
  fccu_state: number;
  fccu_main_valve: boolean;
  fccu_purge_valve: boolean;
  fccu_fan_state: boolean;
  fccu_fan_duty: number;
  fccu_version: string;
  h2_flow: number;
  h2_lp_pressure: number;
  fc_w: number;
  energy_wh: number;
  mcu_status: number;
  vehicle_type: number;
  motor_controller_enabled: boolean;
  main_valve_enabled: boolean;
}

export interface FccuSettings {
  fanMode: number;
  purgeInterval: number;
  pGain: number;
  iGain: number;
  dGain: number;
}

export interface LightingState {
  headlight: boolean;
  positionLight: boolean;
  brakeLight: boolean;
  leftIndicator: boolean;
  rightIndicator: boolean;
  hazard: boolean;
}
