import { test, expect, type Page } from '@playwright/test';

interface MockTauriOptions {
  dbConnected?: boolean;
  canConnected?: boolean;
}

/**
 * Installs an in-page Tauri v2 IPC mock (`window.__TAURI_INTERNALS__`) so the compiled
 * SvelteKit app works in a standard Chromium/Edge browser without native hardware/DB.
 */
async function installTauriMock(page: Page, initial: MockTauriOptions = {}) {
  await page.addInitScript((opts) => {
    let nextCallbackId = 1;
    let dbConfig = {
      host: 'localhost',
      port: 5432,
      user: 'postgres',
      password: 'admin',
      dbname: 'engine_db',
    };
    let dbStatus = {
      connected: Boolean(opts.dbConnected),
      message: opts.dbConnected ? 'Połączono (engine_db)' : 'Brak połączenia',
    };
    let canIsConnected = Boolean(opts.canConnected);

    const listeners: Record<string, Array<{ id: number; handlerId: number }>> = {};
    const callbacks: Record<number, (data: unknown) => void> = {};
    const invokedCalls: Array<{ cmd: string; args: unknown }> = [];

    function emit(event: string, payload: unknown) {
      for (const entry of listeners[event] || []) {
        const cb = callbacks[entry.handlerId];
        if (cb) cb({ event, payload });
      }
    }

    (window as any).__mockTauri = {
      emit,
      getInvokedCalls: () => invokedCalls,
      setCanConnected: (val: boolean) => {
        canIsConnected = val;
        emit('can-connection-status', val);
      },
      setDbStatus: (connected: boolean, message: string) => {
        dbStatus = { connected, message };
        emit('db-connection-status', dbStatus);
      },
    };

    (window as any).__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: 'main' },
        currentWebview: { label: 'main', windowLabel: 'main' },
      },
      transformCallback: (callback: (data: unknown) => void, once = false) => {
        const id = nextCallbackId++;
        callbacks[id] = (data: unknown) => {
          if (once) delete callbacks[id];
          callback(data);
        };
        return id;
      },
      unregisterCallback: (id: number) => {
        delete callbacks[id];
      },
      invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
        invokedCalls.push({ cmd, args });

        if (cmd === 'plugin:event|listen') {
          const eventName = args.event as string;
          const handlerId = args.handler as number;
          const listenerId = nextCallbackId++;
          if (!listeners[eventName]) listeners[eventName] = [];
          listeners[eventName].push({ id: listenerId, handlerId });
          return listenerId;
        }
        if (cmd === 'plugin:event|unlisten') {
          const eventName = args.event as string;
          const eventId = args.eventId as number;
          listeners[eventName] = (listeners[eventName] || []).filter((e) => e.id !== eventId);
          return null;
        }

        switch (cmd) {
          case 'get_db_config':
            return dbConfig;
          case 'get_db_status':
            return dbStatus;
          case 'save_db_config':
            dbConfig = args.config as typeof dbConfig;
            return 'Dane zapisane pomyślnie. Trwa łączenie z bazą...';
          case 'get_can_status':
            return canIsConnected;
          case 'reconnect_pcan':
            if (canIsConnected) return 'Połączono z adapterem PCAN-USB (kanał 0x51)';
            throw 'Nie wykryto adaptera PCAN-USB';
          case 'send_fccu_command':
            return `${args.commandType} → ${args.state ? 'ON' : 'OFF'}`;
          case 'send_fccu_settings':
            return 'Wysłano FCCU Settings';
          case 'send_lighting':
            return 'Wysłano Lighting';
          default:
            return null;
        }
      },
    };
  }, initial);
}

test.describe('Engine Diag Functional E2E Suite', () => {
  test('Grafana tab: charts disabled without DB, user enters credentials and receives live telemetry', async ({
    page,
  }) => {
    await installTauriMock(page, { dbConnected: false, canConnected: false });
    await page.goto('/');

    // 1. Charts start disabled while DB config card remains enabled
    const chartsWrapper = page.locator('.charts-area-wrapper');
    await expect(chartsWrapper).toHaveClass(/disabled/);
    await expect(page.getByText('Brak aktywnego połączenia z bazą danych (PostgreSQL)')).toBeVisible();

    const hostInput = page.locator('#db-host');
    await expect(hostInput).toBeEnabled();
    await hostInput.fill('10.0.0.15');
    await page.locator('#db-dbname').fill('telemetry_prod');
    await page.getByRole('button', { name: /Zastosuj i połącz z bazą/i }).click();

    // Verify backend IPC received updated DB credentials
    const calls = await page.evaluate(() => (window as any).__mockTauri.getInvokedCalls());
    expect(calls.some((c: any) => c.cmd === 'save_db_config' && c.args.config.host === '10.0.0.15')).toBe(
      true
    );

    // 2. Backend establishes connection and streams telemetry
    await page.evaluate(() => {
      (window as any).__mockTauri.setDbStatus(true, 'Połączono (telemetry_prod)');
      (window as any).__mockTauri.emit('engine-data-update', {
        voltage1: 48.5,
        voltage2: 47.2,
        voltage3: 46.8,
        current1: 11.4,
        current2: 5.0,
        current3: 6.4,
        power1: 553,
        power2: 236,
        power3: 300,
        temperature: 58.3,
        rpm: 2450,
      });
    });

    await expect(chartsWrapper).not.toHaveClass(/disabled/);
    await expect(page.locator('.can-comm-box')).toContainText('Połączono');
    await expect(page.locator('.sidebar')).toContainText('48,5 V');
    await expect(page.locator('.sidebar')).toContainText('2450 obr./min');
  });

  test('CAN tab: USB auto-listening disabled state, hotplug unlock, PID toggle, valve lock/unlock, and unplug reset', async ({
    page,
  }) => {
    await installTauriMock(page, { dbConnected: false, canConnected: false });
    await page.goto('/can');

    const canWrapper = page.locator('.can-view-wrapper');

    // 1. Disconnected USB -> entire CAN view disabled + auto-listening indicator visible
    await expect(canWrapper).toHaveClass(/disabled/);
    await expect(page.getByText('Nasłuchiwanie USB w toku...')).toBeVisible();
    await expect(page.locator('.can-comm-box')).toContainText('Rozłączono');

    // 2. Simulate USB cable plugged in -> page automatically unlocks
    await page.evaluate(() => {
      (window as any).__mockTauri.setCanConnected(true);
    });
    await expect(canWrapper).not.toHaveClass(/disabled/);
    await expect(page.locator('.can-comm-box')).toContainText('Połączono');

    // Sidebar: U_SC and U_MC must remain disabled on CAN tab
    const uScCard = page.locator('.metric-card', { hasText: 'U_SC' });
    const uMcCard = page.locator('.metric-card', { hasText: 'U_MC' });
    const uFcCard = page.locator('.metric-card', { hasText: 'U_FC' });
    await expect(uScCard).toHaveClass(/disabled/);
    await expect(uMcCard).toHaveClass(/disabled/);
    await expect(uFcCard).not.toHaveClass(/disabled/);

    // 3. PID controls only enabled when fan mode == Regulator PID (2)
    const pInput = page.locator('#p-gain-input');
    await expect(pInput).toBeDisabled();
    await page.locator('#fan-mode-select').selectOption('2');
    await expect(pInput).toBeEnabled();
    await page.locator('#fan-mode-select').selectOption('0');
    await expect(pInput).toBeDisabled();

    // 4. Pressing "Otwórz zawór główny" disables button until CAN confirms main_valve=true
    const valveBtn = page.getByRole('button', { name: 'Otwórz zawór główny' });
    await valveBtn.click();
    await expect(valveBtn).toBeDisabled();
    await expect(valveBtn).toContainText('Otwieranie...');

    await page.evaluate(() => {
      (window as any).__mockTauri.emit('can-data-update', {
        fccu_state: 2,
        fccu_main_valve: true,
        fccu_purge_valve: false,
        fccu_fan_state: true,
        fccu_fan_duty: 40,
        fccu_version: 'v2.1.0',
        fc_output_voltage: 49.1,
        fc_output_current: 12.0,
        fc_w: 589,
        fc_temp: 52.4,
        rpm: 1800,
      });
    });

    await expect(page.getByRole('button', { name: 'Zamknij zawór główny' })).toBeEnabled();
    await expect(page.locator('.top-status-bar')).toContainText('Otwarty');
    await expect(page.locator('.top-status-bar')).toContainText('v2.1.0');

    // 5. Simulate unplugging USB cable -> page resets and goes back to disabled
    await page.evaluate(() => {
      (window as any).__mockTauri.setCanConnected(false);
    });

    await expect(canWrapper).toHaveClass(/disabled/);
    await expect(page.locator('.can-comm-box')).toContainText('Rozłączono');
    await expect(uFcCard).toHaveClass(/disabled/);
  });
});
