import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  EngineData,
  CanReadData,
  FccuCommandType,
  FccuSettings,
  LightingState,
} from '$lib/types';

/**
 * Sends a single FCCU toggle command to CAN bus via Tauri backend.
 */
export async function sendFccuCommand(
  commandType: FccuCommandType,
  state: boolean
): Promise<string> {
  return await invoke<string>('send_fccu_command', {
    commandType,
    state,
  });
}

/**
 * Sends FCCU fan and purge valve configuration to CAN bus via Tauri backend.
 */
export async function sendFccuSettings(
  settings: FccuSettings
): Promise<string> {
  return await invoke<string>('send_fccu_settings', {
    fanMode: settings.fanMode,
    purgeInterval: settings.purgeInterval,
    pGain: settings.pGain,
    iGain: settings.iGain,
    dGain: settings.dGain,
  });
}

/**
 * Sends lighting commands to vehicle MCU via Tauri backend.
 */
export async function sendLighting(
  lighting: LightingState
): Promise<string> {
  return await invoke<string>('send_lighting', {
    headlight: lighting.headlight,
    positionLight: lighting.positionLight,
    brakeLight: lighting.brakeLight,
    leftIndicator: lighting.leftIndicator,
    rightIndicator: lighting.rightIndicator,
    hazard: lighting.hazard,
  });
}

/**
 * Subscribes to engine data stream from PostgreSQL / TimescaleDB.
 */
export async function subscribeEngineData(
  callback: (data: EngineData) => void
): Promise<UnlistenFn> {
  return await listen<EngineData>('engine-data-update', (event) => {
    callback(event.payload);
  });
}

/**
 * Subscribes to live CAN bus stream.
 */
export async function subscribeCanData(
  callback: (data: CanReadData) => void
): Promise<UnlistenFn> {
  return await listen<CanReadData>('can-data-update', (event) => {
    callback(event.payload);
  });
}
