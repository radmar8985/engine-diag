<script lang="ts">
  import { onMount } from 'svelte';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import Plotly from 'plotly.js-dist-min';
  import { baseLayout, plotlyOptions, MAX_POINTS } from '$lib/chartUtils';
  import { dbStatus } from '$lib/stores';
  import type { EngineData, DbConfig, DbStatus } from '$lib/types';

  let voltChart: HTMLElement;
  let currChart: HTMLElement;
  let tempChart: HTMLElement;
  let rpmChart: HTMLElement;
  let powerChart: HTMLElement;
  let grafanaChartsInitialized = false;

  // DB Configuration form state
  let dbConfig: DbConfig = $state({
    host: 'localhost',
    port: 5432,
    user: 'postgres',
    password: '',
    dbname: 'engine_db'
  });
  let showPassword = $state(false);
  let isSaving = $state(false);
  let saveFeedback = $state<{ type: 'success' | 'error'; message: string } | null>(null);
  let isConfigExpanded = $state(true);

  const grafanaCharts = {
    voltages: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_FC (V)', line: { color: '#3b82f6', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_SC (V)', line: { color: '#60a5fa', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'U_MC (V)', line: { color: '#93c5fd', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Napięcie (V)' } }
    },
    currents: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_FC (A)', line: { color: '#10b981', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_SC (A)', line: { color: '#34d399', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'I_MC (A)', line: { color: '#6ee7b7', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Prąd (A)' } }
    },
    temperature: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Temperatura (°C)', line: { color: '#f59e0b', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Temp (°C)' } }
    },
    rpm: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'RPM', line: { color: '#8b5cf6', width: 2 } }],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'RPM' } }
    },
    power: {
      data: [
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_FC (W)', line: { color: '#ec4899', width: 2 } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_SC (W)', line: { color: '#f472b6', width: 2, dash: 'dot' } },
        { x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'P_MC (W)', line: { color: '#f9a8d4', width: 2, dash: 'dash' } }
      ],
      layout: { ...baseLayout, yaxis: { ...baseLayout.yaxis, title: 'Moc (W)' } }
    }
  };

  async function handleSaveDbConfig() {
    isSaving = true;
    saveFeedback = null;
    try {
      const res = await invoke<string>('save_db_config', { config: dbConfig });
      try {
        localStorage.setItem('engine_db_config', JSON.stringify(dbConfig));
      } catch (e) {
        console.warn('Nie udało się zapisać konfiguracji do localStorage:', e);
      }
      saveFeedback = { type: 'success', message: res };
      setTimeout(() => {
        if (saveFeedback?.type === 'success') saveFeedback = null;
      }, 5000);
    } catch (err: any) {
      saveFeedback = {
        type: 'error',
        message: typeof err === 'string' ? err : (err?.message || 'Nieznany błąd zapisu')
      };
    } finally {
      isSaving = false;
    }
  }

  onMount(() => {
    if (voltChart) {
      Plotly.newPlot(voltChart, grafanaCharts.voltages.data, grafanaCharts.voltages.layout, plotlyOptions);
      Plotly.newPlot(currChart, grafanaCharts.currents.data, grafanaCharts.currents.layout, plotlyOptions);
      Plotly.newPlot(tempChart, grafanaCharts.temperature.data, grafanaCharts.temperature.layout, plotlyOptions);
      Plotly.newPlot(rpmChart, grafanaCharts.rpm.data, grafanaCharts.rpm.layout, plotlyOptions);
      Plotly.newPlot(powerChart, grafanaCharts.power.data, grafanaCharts.power.layout, plotlyOptions);
      grafanaChartsInitialized = true;
    }

    try {
      const cached = localStorage.getItem('engine_db_config');
      if (cached) {
        const parsed = JSON.parse(cached);
        dbConfig = { ...dbConfig, ...parsed };
      }
    } catch (e) {
      console.warn('Błąd odczytu konfiguracji bazy z localStorage:', e);
    }

    invoke<DbConfig>('get_db_config')
      .then((cfg) => {
        if (cfg) {
          dbConfig = cfg;
        }
      })
      .catch((e) => console.error('get_db_config error:', e));

    invoke<DbStatus>('get_db_status')
      .then((status) => {
        if (status) {
          dbStatus.set(status);
        }
      })
      .catch((e) => console.error('get_db_status error:', e));

    const unlistenPg = listen<EngineData>('engine-data-update', (event) => {
      const data = event.payload;
      const now = new Date().toLocaleTimeString();

      if (grafanaChartsInitialized && voltChart) {
        Plotly.extendTraces(voltChart, { x: [[now], [now], [now]], y: [[data.voltage1], [data.voltage2], [data.voltage3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(currChart, { x: [[now], [now], [now]], y: [[data.current1], [data.current2], [data.current3]] }, [0, 1, 2], MAX_POINTS);
        Plotly.extendTraces(tempChart, { x: [[now]], y: [[data.temperature]] }, [0], MAX_POINTS);
        Plotly.extendTraces(rpmChart, { x: [[now]], y: [[data.rpm]] }, [0], MAX_POINTS);
        Plotly.extendTraces(powerChart, { x: [[now], [now], [now]], y: [[data.power1], [data.power2], [data.power3]] }, [0, 1, 2], MAX_POINTS);
      }
    });

    const unlistenDbStatus = listen<DbStatus>('db-connection-status', (event) => {
      dbStatus.set(event.payload);
    });

    return () => {
      unlistenPg.then((u) => u());
      unlistenDbStatus.then((u) => u());
    };
  });
</script>

<div class="db-config-card">
  <div class="db-config-header">
    <div class="db-config-header-left">
      <span class="db-config-title">Konfiguracja połączenia z bazą (PostgreSQL)</span>
      <div class="db-status-badge {$dbStatus.connected ? 'status-connected' : 'status-disconnected'}">
        <span class="status-dot"></span>
        <span class="status-text">{$dbStatus.connected ? 'Połączono' : 'Rozłączono'}: {$dbStatus.message}</span>
      </div>
    </div>
    <button
      type="button"
      class="btn-toggle-config"
      onclick={() => (isConfigExpanded = !isConfigExpanded)}
      aria-label="Rozwiń lub zwiń konfigurację bazy"
    >
      {isConfigExpanded ? 'Zwiń ▲' : 'Ustawienia połączenia ▼'}
    </button>
  </div>

  {#if isConfigExpanded}
    <form class="db-config-form" onsubmit={(e) => { e.preventDefault(); handleSaveDbConfig(); }}>
      <div class="db-form-grid">
        <div class="db-input-group">
          <label for="db-host">Host</label>
          <input
            id="db-host"
            type="text"
            bind:value={dbConfig.host}
            placeholder="np. localhost lub IP"
            required
          />
        </div>

        <div class="db-input-group">
          <label for="db-port">Port</label>
          <input
            id="db-port"
            type="number"
            bind:value={dbConfig.port}
            placeholder="5432"
            min="1"
            max="65535"
            required
          />
        </div>

        <div class="db-input-group">
          <label for="db-dbname">Baza danych</label>
          <input
            id="db-dbname"
            type="text"
            bind:value={dbConfig.dbname}
            placeholder="engine_db"
            required
          />
        </div>

        <div class="db-input-group">
          <label for="db-user">Użytkownik</label>
          <input
            id="db-user"
            type="text"
            bind:value={dbConfig.user}
            placeholder="postgres"
            required
          />
        </div>

        <div class="db-input-group">
          <label for="db-password">Hasło</label>
          <div class="password-input-wrapper">
            <input
              id="db-password"
              type={showPassword ? 'text' : 'password'}
              bind:value={dbConfig.password}
              placeholder="Wprowadź hasło"
            />
            <button
              type="button"
              class="btn-show-password"
              onclick={() => (showPassword = !showPassword)}
              title={showPassword ? 'Ukryj hasło' : 'Pokaż hasło'}
            >
              {showPassword ? '👁' : '🔒'}
            </button>
          </div>
        </div>
      </div>

      <div class="db-form-actions">
        <button type="submit" class="btn-save-db" disabled={isSaving}>
          {isSaving ? 'Zapisywanie i łączenie...' : 'Zastosuj i połącz z bazą'}
        </button>

        {#if saveFeedback}
          <div class="feedback-msg feedback-{saveFeedback.type}">
            {saveFeedback.message}
          </div>
        {/if}
      </div>
    </form>
  {/if}
</div>

<div class="chart-container"><h3 class="chart-title">Napięcia (U_FC, U_SC, U_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={voltChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Prądy (I_FC, I_SC, I_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={currChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Temperatura</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={tempChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Prędkość obrotowa (RPM)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={rpmChart}></div></div></div>
<div class="chart-container"><h3 class="chart-title">Moc (P_FC, P_SC, P_MC)</h3><div class="canvas-wrapper"><div class="plotly-chart" bind:this={powerChart}></div></div></div>

<style>
  .db-config-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 14px 18px;
    margin-bottom: 4px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .db-config-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .db-config-header-left {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }
  .db-config-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #e4e4e7;
  }
  .db-status-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 9999px;
    font-size: 0.78rem;
    font-weight: 500;
    background-color: #27272a;
    color: #a1a1aa;
    max-width: 500px;
  }
  .db-status-badge.status-connected {
    background-color: rgba(34, 197, 94, 0.15);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.3);
  }
  .db-status-badge.status-disconnected {
    background-color: rgba(239, 68, 68, 0.12);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.25);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: currentColor;
    flex-shrink: 0;
  }
  .status-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .btn-toggle-config {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    color: #d4d4d8;
    padding: 5px 12px;
    border-radius: 6px;
    font-size: 0.8rem;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .btn-toggle-config:hover {
    background-color: #3f3f46;
    color: #ffffff;
  }
  .db-config-form {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 8px;
    border-top: 1px solid #27272a;
  }
  .db-form-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 12px;
  }
  .db-input-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .db-input-group label {
    font-size: 0.75rem;
    font-weight: 500;
    color: #a1a1aa;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .db-input-group input {
    background-color: #09090b;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    padding: 7px 10px;
    color: #f4f4f5;
    font-size: 0.85rem;
    outline: none;
    transition: border-color 0.15s;
    width: 100%;
    box-sizing: border-box;
  }
  .db-input-group input:focus {
    border-color: #3b82f6;
  }
  .password-input-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }
  .password-input-wrapper input {
    padding-right: 36px;
  }
  .btn-show-password {
    position: absolute;
    right: 4px;
    background: transparent;
    border: none;
    color: #a1a1aa;
    cursor: pointer;
    font-size: 0.9rem;
    padding: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .btn-show-password:hover {
    color: #ffffff;
  }
  .db-form-actions {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .btn-save-db {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    border: 1px solid #3b82f6;
    color: #ffffff;
    font-weight: 600;
    font-size: 0.84rem;
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .btn-save-db:hover:not(:disabled) {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    box-shadow: 0 0 10px rgba(59, 130, 246, 0.4);
  }
  .btn-save-db:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .feedback-msg {
    font-size: 0.82rem;
    padding: 4px 10px;
    border-radius: 6px;
  }
  .feedback-success {
    background-color: rgba(34, 197, 94, 0.15);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.3);
  }
  .feedback-error {
    background-color: rgba(239, 68, 68, 0.15);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.3);
  }

  .chart-container {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
  }
  .chart-title { font-size: 0.9rem; color: #a1a1aa; margin: 0 0 10px 0; }
  .canvas-wrapper { height: 220px; width: 100%; }
  .plotly-chart { width: 100%; height: 100%; }
</style>
