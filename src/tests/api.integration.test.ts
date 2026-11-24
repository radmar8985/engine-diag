import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
const listenMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));

import {
  sendFccuCommand,
  sendFccuSettings,
  sendLighting,
  subscribeEngineData,
  subscribeCanData,
} from '$lib/services/api';

describe('Tauri IPC API service integration', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
  });

  it('invokes send_fccu_command with exact payload parameters', async () => {
    invokeMock.mockResolvedValueOnce('FCCU_MAIN_VALVE_COMMAND → ON');
    const res = await sendFccuCommand('main_valve', true);

    expect(invokeMock).toHaveBeenCalledWith('send_fccu_command', {
      commandType: 'main_valve',
      state: true,
    });
    expect(res).toBe('FCCU_MAIN_VALVE_COMMAND → ON');
  });

  it('invokes send_fccu_settings with mapped PID and purge fields', async () => {
    invokeMock.mockResolvedValueOnce('OK');
    await sendFccuSettings({
      fanMode: 2,
      purgeInterval: 120,
      pGain: 150,
      iGain: 60,
      dGain: 15,
    });

    expect(invokeMock).toHaveBeenCalledWith('send_fccu_settings', {
      fanMode: 2,
      purgeInterval: 120,
      pGain: 150,
      iGain: 60,
      dGain: 15,
    });
  });

  it('invokes send_lighting with all 6 boolean lighting flags', async () => {
    invokeMock.mockResolvedValueOnce('OK');
    await sendLighting({
      headlight: true,
      positionLight: false,
      brakeLight: true,
      leftIndicator: false,
      rightIndicator: true,
      hazard: false,
    });

    expect(invokeMock).toHaveBeenCalledWith('send_lighting', {
      headlight: true,
      positionLight: false,
      brakeLight: true,
      leftIndicator: false,
      rightIndicator: true,
      hazard: false,
    });
  });

  it('subscribes to engine-data-update and can-data-update events and forwards payloads', async () => {
    const handlers: Record<string, (e: { payload: unknown }) => void> = {};
    const unlistenSpy = vi.fn();

    listenMock.mockImplementation(async (eventName: string, cb: (e: { payload: unknown }) => void) => {
      handlers[eventName] = cb;
      return unlistenSpy;
    });

    const onEngine = vi.fn();
    const onCan = vi.fn();

    const unlistenEngine = await subscribeEngineData(onEngine);
    const unlistenCan = await subscribeCanData(onCan);

    handlers['engine-data-update']({ payload: { voltage1: 48.2, rpm: 1500 } });
    handlers['can-data-update']({ payload: { fc_output_voltage: 47.9, fccu_state: 2 } });

    expect(onEngine).toHaveBeenCalledWith({ voltage1: 48.2, rpm: 1500 });
    expect(onCan).toHaveBeenCalledWith({ fc_output_voltage: 47.9, fccu_state: 2 });

    unlistenEngine();
    unlistenCan();
    expect(unlistenSpy).toHaveBeenCalledTimes(2);
  });
});
