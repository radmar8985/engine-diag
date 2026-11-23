import { writable } from 'svelte/store';
import type { EngineData, DbStatus } from './types';

export const currentReadings = writable<EngineData>({
  voltage1: 0,
  voltage2: 0,
  voltage3: 0,
  current1: 0,
  current2: 0,
  current3: 0,
  temperature: 0,
  rpm: 0,
  power1: 0,
  power2: 0,
  power3: 0
});

export const canConnected = writable<boolean>(false);
export const lastCanFrameTime = writable<string>('---');

export const dbStatus = writable<DbStatus>({
  connected: false,
  message: 'Inicjalizacja...'
});
