import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { createRawSnippet } from 'svelte';
type EventHandler = (event: { payload: unknown }) => void;

const { listeners, invokeMock, pageStore } = vi.hoisted(() => {
  type PageVal = { url: URL };
  let currentPage: PageVal = { url: new URL('http://localhost/') };
  const pageSubs = new Set<(val: PageVal) => void>();

  return {
    listeners: {} as Record<string, EventHandler[]>,
    invokeMock: vi.fn(),
    pageStore: {
      subscribe(fn: (val: PageVal) => void) {
        fn(currentPage);
        pageSubs.add(fn);
        return () => pageSubs.delete(fn);
      },
      set(val: PageVal) {
        currentPage = val;
        for (const fn of pageSubs) fn(currentPage);
      },
    },
  };
});

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, cb: EventHandler) => {
    if (!listeners[name]) listeners[name] = [];
    listeners[name].push(cb);
    return () => {
      listeners[name] = (listeners[name] || []).filter((fn) => fn !== cb);
    };
  }),
}));

vi.mock('$app/stores', () => ({
  page: pageStore,
}));

vi.mock('plotly.js-dist-min', () => ({
  default: {
    newPlot: vi.fn(),
    extendTraces: vi.fn(),
    update: vi.fn(),
    relayout: vi.fn(),
  },
}));

function emitTauriEvent(name: string, payload: unknown) {
  for (const cb of listeners[name] ?? []) {
    cb({ payload });
  }
}

import GrafanaPage from '../routes/grafana/+page.svelte';
import CanPage from '../routes/can/+page.svelte';
import RootLayout from '../routes/+layout.svelte';
import { canConnected, canConnectionInfo, dbStatus } from '$lib/stores';

describe('Svelte component integration tests', () => {
  beforeEach(() => {
    for (const k of Object.keys(listeners)) delete listeners[k];
    invokeMock.mockReset();
    localStorage.clear();
    canConnected.set(false);
    canConnectionInfo.set({
      connected: false,
      channel: null,
      channel_label: 'Rozłączono',
      baudrate_label: '500 kbit/s',
      manual_disconnected: false,
    });
    dbStatus.set({ connected: false, message: 'Rozłączono' });
    pageStore.set({ url: new URL('http://localhost/') });
  });

  describe('Grafana view (+page.svelte)', () => {
    it('disables charts when DB is disconnected while keeping credentials form enabled', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_db_config') {
          return { host: 'localhost', port: 5432, user: 'postgres', password: 'secret', dbname: 'engine_db' };
        }
        if (cmd === 'get_db_status') {
          return { connected: false, message: 'Brak połączenia' };
        }
        if (cmd === 'save_db_config') {
          return 'Dane zapisane pomyślnie';
        }
        return null;
      });

      const { container } = render(GrafanaPage);

      await waitFor(() => {
        const wrapper = container.querySelector('.charts-area-wrapper');
        expect(wrapper?.classList.contains('disabled')).toBe(true);
      });

      expect(screen.getByText('Brak aktywnego połączenia z bazą danych (PostgreSQL)')).toBeTruthy();

      // Credentials inputs must stay editable even when charts are disabled
      const hostInput = screen.getByLabelText('Host') as HTMLInputElement;
      expect(hostInput.disabled).toBe(false);
      await fireEvent.input(hostInput, { target: { value: '192.168.1.50' } });

      const submitBtn = screen.getByRole('button', { name: /Zastosuj i połącz z bazą/i });
      expect((submitBtn as HTMLButtonElement).disabled).toBe(false);
      await fireEvent.click(submitBtn);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('save_db_config', {
          config: expect.objectContaining({ host: '192.168.1.50' }),
        });
      });

      // When DB connects via event, charts unlock and Disconnect button appears
      emitTauriEvent('db-connection-status', { connected: true, message: 'Połączono (engine_db @ 127.0.0.1:5432)' });
      await waitFor(() => {
        const wrapper = container.querySelector('.charts-area-wrapper');
        expect(wrapper?.classList.contains('disabled')).toBe(false);
      });

      const disconnectBtn = screen.getByRole('button', { name: 'Rozłącz' });
      await fireEvent.click(disconnectBtn);
      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('disconnect_db');
      });
    });
  });

  describe('CAN view (/can/+page.svelte)', () => {
    it('disables entire CAN page when USB is disconnected and enables on hotplug event', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_can_status') return false;
        return null;
      });

      const { container } = render(CanPage);

      await waitFor(() => {
        expect(container.querySelector('.can-view-wrapper')?.classList.contains('disabled')).toBe(true);
      });
      expect(screen.getByText('Nasłuchiwanie USB w toku...')).toBeTruthy();

      // Simulate USB hotplug detection
      emitTauriEvent('can-connection-status', true);
      await waitFor(() => {
        expect(container.querySelector('.can-view-wrapper')?.classList.contains('disabled')).toBe(false);
      });

      // Simulate cable unplug -> immediately disabled again
      emitTauriEvent('can-connection-status', false);
      await waitFor(() => {
        expect(container.querySelector('.can-view-wrapper')?.classList.contains('disabled')).toBe(true);
      });
    });

    it('disables PID gain inputs unless fanMode is set to Regulator PID (2)', async () => {
      invokeMock.mockResolvedValue(true);
      render(CanPage);

      const pInput = screen.getByLabelText('Wzmocnienie P') as HTMLInputElement;
      const iInput = screen.getByLabelText('Wzmocnienie I') as HTMLInputElement;
      const dInput = screen.getByLabelText('Wzmocnienie D') as HTMLInputElement;
      const modeSelect = screen.getByLabelText('Tryb wentylatora') as HTMLSelectElement;

      // Default mode 0 (Manual) -> PID disabled
      expect(pInput.disabled).toBe(true);
      expect(iInput.disabled).toBe(true);
      expect(dInput.disabled).toBe(true);

      // Switch to mode 2 (Regulator PID) -> PID enabled
      await fireEvent.change(modeSelect, { target: { value: '2' } });
      await waitFor(() => {
        expect(pInput.disabled).toBe(false);
        expect(iInput.disabled).toBe(false);
        expect(dInput.disabled).toBe(false);
      });

      // Switch to mode 1 (Linear) -> PID disabled again
      await fireEvent.change(modeSelect, { target: { value: '1' } });
      await waitFor(() => {
        expect(pInput.disabled).toBe(true);
      });
    });

    it('locks FCCU action button while waiting for CAN bus confirmation frame', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_can_status') return true;
        if (cmd === 'send_fccu_command') return 'FCCU_MAIN_VALVE_COMMAND → ON';
        return null;
      });

      render(CanPage);

      const openValveBtn = screen.getByRole('button', { name: 'Otwórz zawór główny' }) as HTMLButtonElement;
      await fireEvent.click(openValveBtn);

      // Button becomes disabled waiting for CAN telemetry confirmation
      await waitFor(() => {
        expect(openValveBtn.disabled).toBe(true);
        expect(openValveBtn.textContent).toContain('Otwieranie...');
      });

      // Simulate receiving CAN telemetry confirming valve opened
      emitTauriEvent('can-data-update', {
        fccu_state: 2,
        fccu_main_valve: true,
        fccu_purge_valve: false,
        fccu_fan_state: false,
        fccu_fan_duty: 0,
        fccu_version: 'v1.2.3',
        fc_output_voltage: 48.5,
        fc_output_current: 10.2,
      });

      await waitFor(() => {
        const closeValveBtn = screen.getByRole('button', { name: 'Zamknij zawór główny' }) as HTMLButtonElement;
        expect(closeValveBtn.disabled).toBe(false);
      });
    });

    it('renders H2 flowmeter chart stats and extended DBC telemetry on CAN updates', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_can_status') return true;
        return null;
      });

      const { container, getByTestId } = render(CanPage);

      await waitFor(() => {
        expect(container.querySelector('.can-view-wrapper')?.classList.contains('disabled')).toBe(false);
      });

      emitTauriEvent('can-data-update', {
        fccu_state: 2,
        fccu_main_valve: true,
        fccu_purge_valve: false,
        fccu_fan_state: true,
        fccu_fan_duty: 50,
        fccu_version: 'v2.4.15',
        fccu_build_id: 4660,
        fc_output_voltage: 48.0,
        fc_output_current: 10.0,
        h2_flow: 14.75,
        h2_total_volume: 123.45,
        h2_lp_pressure: 1.85,
        h2_hp_pressure: 290.5,
        fccu_leakage_voltage: 0.42,
        fc_temp: 54.2,
        ambient_temp: 23.1,
        ambient_humidity: 45.6,
        ambient_pressure: 500.2,
        fccu_dcdc_voltage: 24.1,
        fccu_dcdc_current: 3.5,
        fccu_load_current: 9.8,
        fccu_ho_current_1: 1.2,
        fccu_ho_current_2: 2.3,
        fccu_ho_current_3: 3.4,
        speed: 38.5,
        rpm: 2100,
        mcu_active_faults: ['ERROR_FC_V_LOW'],
      });

      await waitFor(() => {
        const flowCard = getByTestId('flowmeter-chart-card');
        expect(flowCard.textContent).toContain('14,75 Ln/min');
        expect(flowCard.textContent).toContain('123,45 Ln');

        const envCard = getByTestId('dbc-hydrogen-env-card');
        expect(envCard.textContent).toContain('1,85 bar');
        expect(envCard.textContent).toContain('290,5 bar');
        expect(envCard.textContent).toContain('54,2 °C');

        const diagCard = getByTestId('dbc-drive-diag-card');
        expect(diagCard.textContent).toContain('38,5 km/h');
        expect(diagCard.textContent).toContain('ERROR_FC_V_LOW');
      });
    });

    it('displays active PCAN channel info and triggers disconnect_pcan on Rozłącz click', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_can_status') return true;
        if (cmd === 'get_can_connection_info') {
          return {
            connected: true,
            channel: 0x51,
            channel_label: 'PCAN_USBBUS1 (0x51)',
            baudrate_label: '500 kbit/s',
            manual_disconnected: false,
          };
        }
        if (cmd === 'disconnect_pcan') return 'Rozłączono adapter PCAN-USB';
        return null;
      });

      const { getByTestId } = render(CanPage);

      await waitFor(() => {
        const bar = getByTestId('can-connection-bar');
        expect(bar.textContent).toContain('PCAN_USBBUS1 (0x51)');
      });

      const disconnectBtn = screen.getByRole('button', { name: 'Rozłącz' });
      await fireEvent.click(disconnectBtn);

      await waitFor(() => {
        expect(invokeMock).toHaveBeenCalledWith('disconnect_pcan');
      });
    });
  });

  describe('Root Layout sidebar (+layout.svelte)', () => {
    const emptyChildren = createRawSnippet(() => ({
      render: () => '<div data-testid="slot-content"></div>',
    }));

    it('disables U_SC, U_MC, I_SC, I_MC, P_SC, P_MC on /can and disables all cards on disconnect', async () => {
      pageStore.set({ url: new URL('http://localhost/can') });
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_can_status') return true;
        if (cmd === 'get_db_status') return { connected: false, message: 'Rozłączono' };
        return null;
      });

      const { container } = render(RootLayout, { props: { children: emptyChildren } });

      await waitFor(() => {
        expect(screen.getByText('Połączono')).toBeTruthy();
      });

      // On /can, U_FC is active while U_SC and U_MC are always disabled
      const cards = Array.from(container.querySelectorAll('.metric-card'));
      const cardByLabel = (lbl: string) =>
        cards.find((el) => el.querySelector('.metric-label')?.textContent === lbl);

      expect(cardByLabel('U_FC')?.classList.contains('disabled')).toBe(false);
      expect(cardByLabel('U_SC')?.classList.contains('disabled')).toBe(true);
      expect(cardByLabel('U_MC')?.classList.contains('disabled')).toBe(true);
      expect(cardByLabel('I_SC')?.classList.contains('disabled')).toBe(true);
      expect(cardByLabel('P_MC')?.classList.contains('disabled')).toBe(true);

      // Simulate USB disconnect -> even U_FC turns disabled and status becomes Rozłączono
      emitTauriEvent('can-connection-status', false);
      await waitFor(() => {
        expect(cardByLabel('U_FC')?.classList.contains('disabled')).toBe(true);
        expect(screen.getByText('Rozłączono')).toBeTruthy();
      });
    });
  });
});
