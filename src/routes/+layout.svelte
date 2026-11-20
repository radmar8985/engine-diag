<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { listen } from '@tauri-apps/api/event';
  import { currentReadings, canConnected, lastCanFrameTime } from '$lib/stores';
  import { fmtVal, getStatus } from '$lib/chartUtils';
  import type { EngineData, CanReadData } from '$lib/types';

  let { children } = $props();

  const thresholds = {
    voltage: { min: 40, max: 52 },
    current: { min: 0, max: 15 },
    temperature: { min: 0, max: 75 },
    rpm: { min: 0, max: 6500 },
    power: { min: 0, max: 1000 }
  };

  onMount(() => {
    const unlistenPg = listen<EngineData>('engine-data-update', (event) => {
      if ($page.url.pathname !== '/can') {
        currentReadings.set(event.payload);
      }
    });

    const unlistenCan = listen<CanReadData>('can-data-update', (event) => {
      const d = event.payload;
      canConnected.set(true);
      lastCanFrameTime.set(new Date().toLocaleTimeString());

      if ($page.url.pathname === '/can') {
        currentReadings.set({
          voltage1: d.fc_output_voltage ?? 0,
          voltage2: d.sc_voltage ?? 0,
          voltage3: d.mc_supply_voltage ?? 0,
          current1: d.fc_output_current ?? 0,
          current2: d.sc_current ?? 0,
          current3: d.mc_supply_current ?? 0,
          power1: d.fc_w ?? 0,
          power2: (d.sc_voltage ?? 0) * (d.sc_current ?? 0),
          power3: (d.mc_supply_voltage ?? 0) * (d.mc_supply_current ?? 0),
          temperature: d.fc_temp ?? 0,
          rpm: d.rpm ?? 0
        });
      }
    });

    return () => {
      unlistenPg.then((u) => u());
      unlistenCan.then((u) => u());
    };
  });
</script>

<div class="dashboard">
  <aside class="sidebar">
    <h1 class="logo">Monitor magistrali CAN</h1>

    <div class="section">
      <h2 class="section-title">Źródło danych</h2>
      <div class="source-buttons">
        <a
          href="/"
          class="source-btn {$page.url.pathname === '/' || $page.url.pathname === '/grafana' ? 'active' : ''}"
        >
          Grafana
        </a>
        <a
          href="/can"
          class="source-btn {$page.url.pathname === '/can' ? 'active' : ''}"
        >
          CAN
        </a>
      </div>
    </div>

    <div class="section">
      <h2 class="section-title">Pomiary na żywo</h2>

      <div class="metric-group">
        <div class="metric-card status-{getStatus($currentReadings.voltage1, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">U_FC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.voltage1, 1)} <span class="unit">V</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.voltage2, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">U_SC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.voltage2, 1)} <span class="unit">V</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.voltage3, thresholds.voltage)}">
          <div class="metric-header"><span class="metric-label">U_MC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.voltage3, 1)} <span class="unit">V</span></div>
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-card status-{getStatus($currentReadings.current1, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">I_FC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.current1, 1)} <span class="unit">A</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.current2, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">I_SC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.current2, 1)} <span class="unit">A</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.current3, thresholds.current)}">
          <div class="metric-header"><span class="metric-label">I_MC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.current3, 1)} <span class="unit">A</span></div>
        </div>
      </div>

      <div class="metric-group">
        <div class="metric-card status-{getStatus($currentReadings.power1, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">P_FC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.power1, 0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.power2, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">P_SC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.power2, 0)} <span class="unit">W</span></div>
        </div>
        <div class="metric-card status-{getStatus($currentReadings.power3, thresholds.power)}">
          <div class="metric-header"><span class="metric-label">P_MC</span><div class="indicator"></div></div>
          <div class="metric-value">{fmtVal($currentReadings.power3, 0)} <span class="unit">W</span></div>
        </div>
      </div>

      <div class="metric-card status-{getStatus($currentReadings.temperature, thresholds.temperature)}">
        <div class="metric-header"><span class="metric-label">Temperatura</span><div class="indicator"></div></div>
        <div class="metric-value">{fmtVal($currentReadings.temperature, 1)} <span class="unit">°C</span></div>
      </div>

      <div class="metric-card status-{getStatus($currentReadings.rpm, thresholds.rpm)}">
        <div class="metric-header"><span class="metric-label">Prędkość obrotowa</span><div class="indicator"></div></div>
        <div class="metric-value">{fmtVal($currentReadings.rpm, 0)} <span class="unit">obr./min</span></div>
      </div>
    </div>

    <div class="section can-comm-section">
      <h2 class="section-title">Komunikacja CAN</h2>
      <div class="can-comm-box">
        <div class="can-comm-row">
          <span class="can-comm-label">Połączenie</span>
          <span class="can-comm-value">{$canConnected ? 'Połączono' : 'Brak informacji'}</span>
        </div>
        <div class="can-comm-row">
          <span class="can-comm-label">Ostatnia ramka</span>
          <span class="can-comm-value">{$lastCanFrameTime}</span>
        </div>
      </div>
    </div>
  </aside>

  <main class="charts-area">
    {@render children()}
  </main>
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    background-color: #09090b;
    color: #e4e4e7;
    font-family: 'Segoe UI', system-ui, sans-serif;
    height: 100vh;
    overflow: hidden;
  }

  .dashboard {
    display: flex;
    height: 100vh;
    width: 100vw;
  }

  .sidebar {
    width: 260px;
    background-color: #121215;
    border-right: 1px solid #27272a;
    display: flex;
    flex-direction: column;
    padding: 20px 16px;
    gap: 20px;
    overflow-y: auto;
    flex-shrink: 0;
  }

  .logo {
    font-size: 1.1rem;
    font-weight: 700;
    color: #ffffff;
    margin: 0;
    padding-bottom: 12px;
    border-bottom: 1px solid #27272a;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-title {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #71717a;
    font-weight: 600;
    margin: 0;
  }

  .source-buttons {
    display: flex;
    flex-direction: column;
    gap: 8px;
    background-color: #18181b;
    padding: 4px;
    border-radius: 8px;
    border: 1px solid #27272a;
  }

  .source-btn {
    flex: 1;
    padding: 8px 12px;
    border-radius: 6px;
    border: none;
    background: transparent;
    color: #a1a1aa;
    font-size: 0.85rem;
    font-weight: 500;
    cursor: pointer;
    transition: all 0.2s;
    text-align: center;
    text-decoration: none;
    display: block;
  }

  .source-btn:hover {
    color: #ffffff;
  }

  .source-btn.active {
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 600;
  }

  .metric-group {
    display: flex;
    gap: 8px;
  }

  .metric-group :global(.metric-card) {
    flex: 1;
    min-width: 0;
    padding: 8px 10px;
  }

  .metric-group :global(.metric-label) {
    font-size: 0.7rem;
  }

  .metric-group :global(.metric-value) {
    font-size: 0.95rem;
  }

  .metric-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .metric-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .metric-label {
    font-size: 0.75rem;
    color: #a1a1aa;
  }

  .metric-value {
    font-size: 1.15rem;
    font-weight: 600;
    color: #ffffff;
  }

  .unit {
    font-size: 0.75rem;
    color: #71717a;
    font-weight: 400;
  }

  .indicator {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .status-normal .indicator { background-color: #22c55e; }
  .status-warning .indicator { background-color: #f59e0b; }
  .status-alarm .indicator { background-color: #ef4444; }

  .can-comm-section {
    margin-top: auto;
  }

  .can-comm-box {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.75rem;
  }

  .can-comm-row {
    display: flex;
    justify-content: space-between;
    color: #a1a1aa;
  }

  .can-comm-value {
    color: #ffffff;
    font-weight: 500;
  }

  .charts-area {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
</style>
