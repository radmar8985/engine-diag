import { describe, it, expect } from 'vitest';
import { get } from 'svelte/store';
import { currentReadings, canConnected, lastCanFrameTime, dbStatus } from './stores';

describe('stores unit tests', () => {
  it('initializes stores with safe defaults and updates reactively', () => {
    expect(get(canConnected)).toBe(false);
    expect(get(dbStatus).connected).toBe(false);
    expect(get(lastCanFrameTime)).toBe('---');
    expect(get(currentReadings).voltage1).toBe(0);

    canConnected.set(true);
    expect(get(canConnected)).toBe(true);
    canConnected.set(false);
  });
});
