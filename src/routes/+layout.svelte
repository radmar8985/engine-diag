<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import { currentReadings, canConnected, lastCanFrameTime, dbStatus, emptyEngineData } from '$lib/stores';
  import { fmtVal, getStatus } from '$lib/chartUtils';
  import type { EngineData, CanReadData, DbStatus } from '$lib/types';

  let { children } = $props();

  let isCanTab = $derived($page.url.pathname === '/can');
  let isCurrentSourceConnected = $derived(
    isCanTab ? $canConnected : $dbStatus.connected
  );

  const thresholds = {
    voltage: { min: 40, max: 52 },
    current: { min: 0, max: 15 },
    temperature: { min: 0, max: 75 },
    rpm: { min: 0, max: 6500 },
    power: { min: 0, max: 1000 }
  };

  interface MetricCardDef {
    label: string;
    key: keyof EngineData;
    unit: string;
    decimals: number;
    threshold: keyof typeof thresholds;
    canActive: boolean;
  }

  const metricGroups: MetricCardDef[][] = [
    [
      { label: 'U_FC', key: 'voltage1', unit: 'V', decimals: 1, threshold: 'voltage', canActive: true },
      { label: 'U_SC', key: 'voltage2', unit: 'V', decimals: 1, threshold: 'voltage', canActive: false },
      { label: 'U_MC', key: 'voltage3', unit: 'V', decimals: 1, threshold: 'voltage', canActive: false }
    ],
    [
      { label: 'I_FC', key: 'current1', unit: 'A', decimals: 1, threshold: 'current', canActive: true },
      { label: 'I_SC', key: 'current2', unit: 'A', decimals: 1, threshold: 'current', canActive: false },
      { label: 'I_MC', key: 'current3', unit: 'A', decimals: 1, threshold: 'current', canActive: false }
    ],
    [
      { label: 'P_FC', key: 'power1', unit: 'W', decimals: 0, threshold: 'power', canActive: true },
      { label: 'P_SC', key: 'power2', unit: 'W', decimals: 0, threshold: 'power', canActive: false },
      { label: 'P_MC', key: 'power3', unit: 'W', decimals: 0, threshold: 'power', canActive: false }
    ]
  ];

  const standaloneMetrics: MetricCardDef[] = [
    { label: 'Temperatura', key: 'temperature', unit: '°C', decimals: 1, threshold: 'temperature', canActive: true },
    { label: 'Prędkość obrotowa', key: 'rpm', unit: 'obr./min', decimals: 0, threshold: 'rpm', canActive: true }
  ];

  function resetCanReadings() {
    lastCanFrameTime.set('—');
    if (isCanTab) {
      currentReadings.set(emptyEngineData());
    }
  }

  function applyCanStatus(connected: boolean) {
    canConnected.set(connected);
    if (!connected) {
      resetCanReadings();
    }
  }

  onMount(() => {
    invoke<boolean>('get_can_status')
      .then(applyCanStatus)
      .catch(() => {});

    invoke<DbStatus>('get_db_status')
      .then((status) => {
        if (status) dbStatus.set(status);
      })
      .catch(() => {});

    const unlistenCanStatus = listen<boolean>('can-connection-status', (event) => applyCanStatus(event.payload));

    const unlistenPg = listen<EngineData>('engine-data-update', (event) => {
      if (!isCanTab) {
        currentReadings.set(event.payload);
      }
    });

    const unlistenCan = listen<CanReadData>('can-data-update', (event) => {
      if (!$canConnected) return;
      const d = event.payload;
      lastCanFrameTime.set(new Date().toLocaleTimeString());

      if (isCanTab) {
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

    const unlistenDb = listen<DbStatus>('db-connection-status', (event) => {
      dbStatus.set(event.payload);
    });

    const unlisteners = [unlistenPg, unlistenCan, unlistenDb, unlistenCanStatus];
    return () => {
      unlisteners.forEach((p) => p.then((u) => u()));
    };
  });
</script>

<div class="dashboard">
  <aside class="sidebar">
    <h1 class="logo">Monitor magistrali CAN</h1>

    <div class="section">
      <h2 class="section-title">Źródło danych</h2>
      <div class="source-buttons">
        <a href="/" class="source-btn {isCanTab || $page.url.pathname !== '/' && $page.url.pathname !== '/grafana' ? '' : 'active'}">
          Grafana
        </a>
        <a href="/can" class="source-btn {isCanTab ? 'active' : ''}">
          CAN
        </a>
      </div>
    </div>

    <div class="section">
      <h2 class="section-title">Pomiary na żywo</h2>

      {#snippet metricCard(m: MetricCardDef)}
        {@const disabled = (!m.canActive && isCanTab) || !isCurrentSourceConnected}
        {@const val = $currentReadings[m.key]}
        <div
          class="metric-card status-{disabled ? 'disabled' : getStatus(val, thresholds[m.threshold])}"
          class:disabled
        >
          <div class="metric-header"><span class="metric-label">{m.label}</span><div class="indicator"></div></div>
          <div class="metric-value">{disabled ? '—' : fmtVal(val, m.decimals)} <span class="unit">{m.unit}</span></div>
        </div>
      {/snippet}

      {#each metricGroups as group}
        <div class="metric-group">
          {#each group as m}
            {@render metricCard(m)}
          {/each}
        </div>
      {/each}

      {#each standaloneMetrics as m}
        {@render metricCard(m)}
      {/each}
    </div>

    <div class="section can-comm-section">
      <h2 class="section-title">{isCanTab ? 'Komunikacja CAN' : 'Baza danych'}</h2>
      <div class="can-comm-box">
        {#if isCanTab}
          <div class="can-comm-row">
            <span class="can-comm-label">Połączenie</span>
            <span class="can-comm-value {$canConnected ? 'status-text-ok' : 'status-text-err'}">
              {$canConnected ? 'Połączono' : 'Rozłączono'}
            </span>
          </div>
          <div class="can-comm-row">
            <span class="can-comm-label">Ostatnia ramka</span>
            <span class="can-comm-value">{$canConnected ? $lastCanFrameTime : '—'}</span>
          </div>
        {:else}
          <div class="can-comm-row">
            <span class="can-comm-label">Połączenie</span>
            <span class="can-comm-value {$dbStatus.connected ? 'status-text-ok' : 'status-text-err'}">
              {$dbStatus.connected ? 'Połączono' : 'Rozłączono'}
            </span>
          </div>
          <div class="can-comm-row">
            <span class="can-comm-label">Status</span>
            <span class="can-comm-value can-comm-desc" title={$dbStatus.message}>
              {$dbStatus.message}
            </span>
          </div>
        {/if}
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
    transition: opacity 0.25s ease, background-color 0.25s ease, border-color 0.25s ease;
  }

  .metric-card.disabled {
    opacity: 0.38;
    background-color: #121214;
    border-color: #202023;
    cursor: not-allowed;
    user-select: none;
  }

  .metric-card.disabled .metric-label {
    color: #52525b;
  }

  .metric-card.disabled .metric-value {
    color: #52525b;
  }

  .metric-card.disabled .indicator {
    background-color: #3f3f46 !important;
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

  .status-text-ok {
    color: #4ade80 !important;
  }

  .status-text-err {
    color: #f87171 !important;
  }

  .can-comm-desc {
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: right;
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
