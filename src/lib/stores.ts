import { writable } from 'svelte/store';
import type { EngineData, DbStatus, CanConnectionInfo } from './types';

export function emptyEngineData(): EngineData {
  return {
    voltage1: 0,
    voltage2: 0,
    voltage3: 0,
    current1: 0,
    current2: 0,
    current3: 0,
    power1: 0,
    power2: 0,
    power3: 0,
    temperature: 0,
    rpm: 0
  };
}

export const currentReadings = writable<EngineData>(emptyEngineData());

export const canConnected = writable<boolean>(false);
export const canConnectionInfo = writable<CanConnectionInfo>({
  connected: false,
  channel: null,
  channel_label: 'Rozłączono',
  baudrate_label: '500 kbit/s',
  manual_disconnected: false
});
export const lastCanFrameTime = writable<string>('---');

export const dbStatus = writable<DbStatus>({
  connected: false,
  message: 'Inicjalizacja...'
});
