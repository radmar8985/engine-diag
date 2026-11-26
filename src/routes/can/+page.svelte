<script lang="ts">
  import { onMount } from 'svelte';
  import { fly } from 'svelte/transition';
  import { listen } from '@tauri-apps/api/event';
  import { invoke } from '@tauri-apps/api/core';
  import Plotly from 'plotly.js-dist-min';
  import {
    baseLayout,
    plotlyOptions,
    CAN_WINDOW_MS,
    CAN_MAX_POINTS,
    CAN_FLOW_MAX_POINTS,
    formatPlotlyTimestamp,
    pruneWindowSamples,
    fmtVal,
    calcAvg,
    calcRms,
    type TimedSample
  } from '$lib/chartUtils';
  import { canConnected, canConnectionInfo } from '$lib/stores';
  import type { CanConnectionInfo, CanReadData, ChartSeriesStats, FccuSettings, LightingState } from '$lib/types';

  let canChart1: HTMLElement;
  let canChart2: HTMLElement;
  let canChart3: HTMLElement;
  let canChartsInitialized = false;

  function buildCanXAxisRange(nowMs: number): [string, string] {
    return [
      formatPlotlyTimestamp(nowMs - CAN_WINDOW_MS),
      formatPlotlyTimestamp(nowMs)
    ];
  }

  const initialRange = buildCanXAxisRange(Date.now());

  const canChartLayout = {
    ...baseLayout,
    margin: { t: 8, r: 8, l: 38, b: 24 },
    showlegend: false,
    xaxis: {
      type: 'date',
      gridcolor: '#27272a',
      zeroline: false,
      tickfont: { color: '#71717a' },
      tickformat: '%H:%M:%S',
      autorange: false,
      range: initialRange
    },
    yaxis: {
      gridcolor: '#27272a',
      zeroline: false,
      tickfont: { color: '#71717a' },
      autorange: true,
      rangemode: 'tozero'
    }
  };

  const canChartConfigs = {
    chart1: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Napięcie FC (V)', line: { color: '#3b82f6', width: 1.5 } }],
      layout: { ...canChartLayout, yaxis: { ...canChartLayout.yaxis, title: '' } }
    },
    chart2: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Prąd FC (A)', line: { color: '#3b82f6', width: 1.5 } }],
      layout: { ...canChartLayout, yaxis: { ...canChartLayout.yaxis, title: '' } }
    },
    chart3: {
      data: [{ x: [] as string[], y: [] as number[], type: 'scatter', mode: 'lines', name: 'Przepływ H₂ (Ln/min)', line: { color: '#06b6d4', width: 1.5 } }],
      layout: { ...canChartLayout, yaxis: { ...canChartLayout.yaxis, title: '' } }
    }
  };

  let canStats = emptyCanStats();
  let chart1Samples: TimedSample[] = [];
  let chart2Samples: TimedSample[] = [];
  let chart3Samples: TimedSample[] = [];

  let hasCanData = false;
  let fccuState = 3;
  let fccuMainValve = false;
  let fccuPurgeValve = false;
  let fccuFanState = false;
  let fccuFanDuty = 0;
  let fccuVersion = '—';
  let fccuBuildId = 0;
  let fccuVehicleType = 0;
  let fccuFuelcellType = 0;

  // Extended DBC telemetry state
  let h2TotalVolume = 0;
  let h2LpPressure = 0;
  let h2HpPressure = 0;
  let h2LeakageVoltage = 0;
  let fcTemp = 0;
  let ambientTemp = 0;
  let ambientHumidity = 0;
  let ambientPressure = 0;

  let fcPowerW = 0;
  let energyWh = 0;
  let fccuDcdcVoltage = 0;
  let fccuDcdcCurrent = 0;
  let fccuLoadCurrent = 0;
  let fccuHoCurrent1 = 0;
  let fccuHoCurrent2 = 0;
  let fccuHoCurrent3 = 0;
  let scVoltage = 0;
  let scCurrent = 0;
  let mcSupplyVoltage = 0;
  let mcSupplyCurrent = 0;
  let abVoltage = 0;
  let abCurrent = 0;

  let speedKmh = 0;
  let rpmVal = 0;
  let accelPedalV = 0;
  let brakePedalV = 0;
  let accelOutputV = 0;
  let brakeOutputV = 0;
  let mcuStatus = 0;
  let vehicleType = 0;
  let vehicleAllowedToMove = false;
  let motorControllerEnabled = false;
  let mainValveEnabled = false;
  let mcuActiveFaults: string[] = [];
  let ccuStatus = 0;
  let swuCanState = 0;
  let lapNumber = 0;
  let lapTimeMs = 0;
  let totalTimeMs = 0;
  let lcuFrontFault = false;
  let lcuBackFault = false;

  const MCU_STATUS_LABELS: Record<number, string> = {
    0: 'INIT',
    1: 'IDLE',
    2: 'STARTING',
    3: 'SHUTTING_DOWN',
    4: 'RUNNING'
  };

  const VEHICLE_TYPE_LABELS: Record<number, string> = {
    0: 'HYDROS_V1',
    1: 'HYDROS_V2',
    2: 'BOLID',
    3: 'HYDRA'
  };

  const FUELCELL_TYPE_LABELS: Record<number, string> = {
    0: 'PROTIUM',
    1: 'NEW_CELL'
  };

  const CCU_STATUS_LABELS: Record<number, string> = {
    0: 'INIT',
    1: 'OPERATIONAL',
    2: 'WARNING',
    3: 'BUS_OFF',
    4: 'DFU'
  };

  const SWU_CAN_STATE_LABELS: Record<number, string> = {
    0: 'ERROR_ACTIVE',
    1: 'ERROR_WARNING',
    2: 'ERROR_PASSIVE',
    3: 'BUS_OFF',
    4: 'STOPPED'
  };

  function formatDurationMs(ms: number): string {
    const totalSec = Math.floor((ms || 0) / 1000);
    const mins = Math.floor(totalSec / 60);
    const secs = totalSec % 60;
    const tenths = Math.floor(((ms || 0) % 1000) / 100);
    return `${String(mins).padStart(2, '0')}:${String(secs).padStart(2, '0')}.${tenths}`;
  }

  const FCCU_STATES: Record<number, { label: string; color: string }> = {
    0: { label: 'Inicjalizacja', color: '#3b82f6' },
    1: { label: 'Uruchamianie', color: '#f59e0b' },
    2: { label: 'Włączony', color: '#22c55e' },
    3: { label: 'Wyłączony', color: '#f59e0b' }
  };

  function getFccuStateLabel(state: number): string {
    return FCCU_STATES[state]?.label ?? 'Brak odczytu';
  }

  function getFccuStateColor(state: number): string {
    return FCCU_STATES[state]?.color ?? '#71717a';
  }

  type FccuCmdKey = 'system_enable' | 'main_valve' | 'manual_purge' | 'fan_on';

  function idlePendingCmds(): Record<FccuCmdKey, boolean> {
    return { system_enable: false, main_valve: false, manual_purge: false, fan_on: false };
  }

  function emptyCanStats(): Record<'chart1' | 'chart2' | 'chart3', ChartSeriesStats> {
    return {
      chart1: { newest: 0, average: 0, rms: 0 },
      chart2: { newest: 0, average: 0, rms: 0 },
      chart3: { newest: 0, average: 0, rms: 0 }
    };
  }

  let pendingCmds = idlePendingCmds();

  let pendingTimeouts: Record<FccuCmdKey, ReturnType<typeof setTimeout> | null> = {
    system_enable: null,
    main_valve: null,
    manual_purge: null,
    fan_on: null
  };

  let expectedStates: Partial<Record<FccuCmdKey, boolean>> = {};

  let isSendingSettings = false;
  let isSendingLighting = false;
  let isSendingFanDuty = false;
  let fanDutyCmd = 100;

  function clampFanDuty(val: number): number {
    const n = Number(val);
    if (!Number.isFinite(n)) return 0;
    return Math.max(0, Math.min(100, Math.round(n)));
  }

  let isReconnectingPcan = false;
  let isDisconnectingPcan = false;

  async function handleReconnectPcan() {
    isReconnectingPcan = true;
    try {
      const res = await invoke<string>('reconnect_pcan');
      showToast(res, false);
      canConnected.set(true);
    } catch (err: any) {
      showToast(typeof err === 'string' ? err : (err?.message || 'Nie wykryto adaptera PCAN-USB'), true);
    } finally {
      isReconnectingPcan = false;
    }
  }

  async function handleDisconnectPcan() {
    isDisconnectingPcan = true;
    try {
      const res = await invoke<string>('disconnect_pcan');
      showToast(res, false);
      applyCanStatus(false);
    } catch (err: any) {
      showToast(typeof err === 'string' ? err : (err?.message || 'Błąd rozłączania PCAN-USB'), true);
    } finally {
      isDisconnectingPcan = false;
    }
  }

  let fccuSettings: FccuSettings = {
    fanMode: 0,
    purgeInterval: 60,
    pGain: 100,
    iGain: 50,
    dGain: 10
  };

  $: isPidEnabled = Number(fccuSettings.fanMode) === 2;

  let lighting: LightingState = {
    headlight: false,
    positionLight: false,
    brakeLight: false,
    leftIndicator: false,
    rightIndicator: false,
    hazard: false
  };

  let toastMessage = '';
  let toastType: 'success' | 'error' = 'success';
  let toastVisible = false;
  let toastTimer: ReturnType<typeof setTimeout> | null = null;

  function showToast(message: string, isError = false) {
    if (toastTimer) clearTimeout(toastTimer);
    toastMessage = message;
    toastType = isError ? 'error' : 'success';
    toastVisible = true;
    toastTimer = setTimeout(() => {
      toastVisible = false;
    }, 2800);
  }

  async function toggleFccuCmd(cmdType: FccuCmdKey) {
    if (pendingCmds[cmdType]) return;

    const currentStates: Record<FccuCmdKey, boolean> = {
      system_enable: fccuState === 2,
      main_valve: fccuMainValve,
      manual_purge: fccuPurgeValve,
      fan_on: fccuFanState
    };
    const targetState = !currentStates[cmdType];

    pendingCmds = { ...pendingCmds, [cmdType]: true };
    expectedStates[cmdType] = targetState;

    if (pendingTimeouts[cmdType]) {
      clearTimeout(pendingTimeouts[cmdType]!);
    }
    pendingTimeouts[cmdType] = setTimeout(() => {
      if (pendingCmds[cmdType]) {
        pendingCmds = { ...pendingCmds, [cmdType]: false };
        delete expectedStates[cmdType];
        showToast('Brak potwierdzenia z magistrali CAN (timeout)', true);
      }
    }, 4000);

    try {
      const result =
        cmdType === 'fan_on'
          ? await invoke('send_fccu_fan', {
              state: targetState,
              duty: targetState ? (clampFanDuty(fanDutyCmd) || 100) : 0
            })
          : await invoke('send_fccu_command', {
              commandType: cmdType,
              state: targetState
            });
      if (cmdType === 'fan_on' && targetState && clampFanDuty(fanDutyCmd) === 0) {
        fanDutyCmd = 100;
      }
      showToast(result as string, false);
    } catch (e) {
      if (pendingTimeouts[cmdType]) {
        clearTimeout(pendingTimeouts[cmdType]!);
      }
      pendingCmds = { ...pendingCmds, [cmdType]: false };
      delete expectedStates[cmdType];
      showToast(`Błąd: ${e}`, true);
    }
  }

  async function sendFanDuty() {
    const duty = clampFanDuty(fanDutyCmd);
    fanDutyCmd = duty;
    isSendingFanDuty = true;
    await invokeWithToast(
      'send_fccu_fan',
      { state: duty > 0, duty },
      () => (isSendingFanDuty = false)
    );
  }

  async function invokeWithToast(command: string, args: Record<string, unknown>, onSettled: () => void) {
    try {
      const result = await invoke(command, args);
      showToast(result as string, false);
    } catch (e) {
      showToast(`Błąd: ${e}`, true);
    } finally {
      setTimeout(onSettled, 500);
    }
  }

  async function sendFccuSettings() {
    isSendingSettings = true;
    await invokeWithToast(
      'send_fccu_settings',
      {
        fanMode: fccuSettings.fanMode,
        purgeInterval: fccuSettings.purgeInterval,
        pGain: fccuSettings.pGain,
        iGain: fccuSettings.iGain,
        dGain: fccuSettings.dGain
      },
      () => (isSendingSettings = false)
    );
  }

  async function sendLightingCmd() {
    isSendingLighting = true;
    await invokeWithToast(
      'send_lighting',
      {
        headlight: lighting.headlight,
        positionLight: lighting.positionLight,
        brakeLight: lighting.brakeLight,
        leftIndicator: lighting.leftIndicator,
        rightIndicator: lighting.rightIndicator,
        hazard: lighting.hazard
      },
      () => (isSendingLighting = false)
    );
  }

  function clearAllPendingTimeouts() {
    (Object.keys(pendingTimeouts) as FccuCmdKey[]).forEach((k) => {
      if (pendingTimeouts[k]) clearTimeout(pendingTimeouts[k]!);
    });
  }

  function resolvePendingCmd(key: FccuCmdKey, currentState: boolean) {
    if (!pendingCmds[key]) return;
    if (expectedStates[key] === undefined || currentState === expectedStates[key]) {
      if (pendingTimeouts[key]) clearTimeout(pendingTimeouts[key]!);
      pendingCmds = { ...pendingCmds, [key]: false };
      delete expectedStates[key];
    }
  }

  function updateChartSeriesBatch(
    chartEl: HTMLElement,
    samples: TimedSample[],
    newPoints: TimedSample[],
    nowMs: number,
    xRange: [string, string],
    maxPoints: number = CAN_MAX_POINTS
  ): { samples: TimedSample[]; stats: ChartSeriesStats } {
    const next = pruneWindowSamples(
      [...samples, ...newPoints],
      nowMs,
      CAN_WINDOW_MS,
      maxPoints
    );
    Plotly.update(
      chartEl,
      { x: [next.map((s) => formatPlotlyTimestamp(s.t))], y: [next.map((s) => s.v)] },
      { 'xaxis.range': xRange },
      [0]
    );
    const values = next.map((s) => s.v);
    const newest = newPoints.length > 0 ? newPoints[newPoints.length - 1].v : (values[values.length - 1] ?? 0);
    return {
      samples: next,
      stats: { newest, average: calcAvg(values), rms: calcRms(values) }
    };
  }

  function updateChartSeries(
    chartEl: HTMLElement,
    samples: TimedSample[],
    val: number,
    nowMs: number,
    xRange: [string, string]
  ): { samples: TimedSample[]; stats: ChartSeriesStats } {
    return updateChartSeriesBatch(chartEl, samples, [{ t: nowMs, v: val }], nowMs, xRange, CAN_MAX_POINTS);
  }

  const statRows: { label: string; key: keyof ChartSeriesStats }[] = [
    { label: 'Bieżąca', key: 'newest' },
    { label: 'Średnia', key: 'average' },
    { label: 'Wartość skuteczna', key: 'rms' }
  ];

  const pidFields: { id: string; key: 'pGain' | 'iGain' | 'dGain'; label: string; helper: string }[] = [
    { id: 'p-gain-input', key: 'pGain', label: 'Wzmocnienie P', helper: 'Część proporcjonalna' },
    { id: 'i-gain-input', key: 'iGain', label: 'Wzmocnienie I', helper: 'Część całkująca' },
    { id: 'd-gain-input', key: 'dGain', label: 'Wzmocnienie D', helper: 'Część różniczkująca' }
  ];

  const lightingColumns: { key: keyof LightingState; label: string }[][] = [
    [
      { key: 'headlight', label: 'Światła główne' },
      { key: 'brakeLight', label: 'Światło STOP' },
      { key: 'rightIndicator', label: 'Prawy kierunkowskaz' }
    ],
    [
      { key: 'positionLight', label: 'Światła pozycyjne' },
      { key: 'leftIndicator', label: 'Lewy kierunkowskaz' },
      { key: 'hazard', label: 'Światła awaryjne' }
    ]
  ];

  $: cmdTiles = [
    {
      key: 'system_enable' as FccuCmdKey,
      name: 'System',
      stateText: fccuState === 2 ? 'włączony' : 'wyłączony',
      btnClass: 'btn-blue',
      active: fccuState === 2,
      pendingOn: 'Włączanie...',
      pendingOff: 'Wyłączanie...',
      labelOn: 'Wyłącz system',
      labelOff: 'Włącz system'
    },
    {
      key: 'main_valve' as FccuCmdKey,
      name: 'Zawór główny',
      stateText: fccuMainValve ? 'otwarty' : 'zamknięty',
      btnClass: 'btn-grey',
      active: fccuMainValve,
      pendingOn: 'Otwieranie...',
      pendingOff: 'Zamykanie...',
      labelOn: 'Zamknij zawór główny',
      labelOff: 'Otwórz zawór główny'
    },
    {
      key: 'manual_purge' as FccuCmdKey,
      name: 'Przedmuch (purge)',
      stateText: fccuPurgeValve ? 'aktywny' : 'nieaktywny',
      btnClass: 'btn-grey',
      active: fccuPurgeValve,
      pendingOn: 'Uruchamianie...',
      pendingOff: 'Zatrzymywanie...',
      labelOn: 'Zatrzymaj przedmuch',
      labelOff: 'Uruchom przedmuch'
    },
    {
      key: 'fan_on' as FccuCmdKey,
      name: 'Wentylator',
      stateText: fccuFanState ? `włączony (${fccuFanDuty}%)` : 'wyłączony',
      btnClass: 'btn-grey',
      active: fccuFanState,
      pendingOn: 'Włączanie...',
      pendingOff: 'Wyłączanie...',
      labelOn: 'Wyłącz wentylator',
      labelOff: 'Włącz wentylator'
    }
  ];

  function resetCanData() {
    hasCanData = false;
    fccuState = 0;
    fccuMainValve = false;
    fccuPurgeValve = false;
    fccuFanState = false;
    fccuFanDuty = 0;
    fccuVersion = '—';
    fccuBuildId = 0;
    fccuVehicleType = 0;
    fccuFuelcellType = 0;
    h2TotalVolume = 0;
    h2LpPressure = 0;
    h2HpPressure = 0;
    h2LeakageVoltage = 0;
    fcTemp = 0;
    ambientTemp = 0;
    ambientHumidity = 0;
    ambientPressure = 0;
    fcPowerW = 0;
    energyWh = 0;
    fccuDcdcVoltage = 0;
    fccuDcdcCurrent = 0;
    fccuLoadCurrent = 0;
    fccuHoCurrent1 = 0;
    fccuHoCurrent2 = 0;
    fccuHoCurrent3 = 0;
    scVoltage = 0;
    scCurrent = 0;
    mcSupplyVoltage = 0;
    mcSupplyCurrent = 0;
    abVoltage = 0;
    abCurrent = 0;
    speedKmh = 0;
    rpmVal = 0;
    accelPedalV = 0;
    brakePedalV = 0;
    accelOutputV = 0;
    brakeOutputV = 0;
    mcuStatus = 0;
    vehicleType = 0;
    vehicleAllowedToMove = false;
    motorControllerEnabled = false;
    mainValveEnabled = false;
    mcuActiveFaults = [];
    ccuStatus = 0;
    swuCanState = 0;
    lapNumber = 0;
    lapTimeMs = 0;
    totalTimeMs = 0;
    lcuFrontFault = false;
    lcuBackFault = false;
    chart1Samples = [];
    chart2Samples = [];
    chart3Samples = [];
    canStats = emptyCanStats();
    clearAllPendingTimeouts();
    pendingCmds = idlePendingCmds();
    expectedStates = {};
  }

  function applyCanStatus(connected: boolean) {
    canConnected.set(connected);
    if (!connected) {
      resetCanData();
    }
  }

  function applyCanInfo(info: CanConnectionInfo) {
    if (!info) return;
    canConnectionInfo.set(info);
    applyCanStatus(info.connected);
  }

  onMount(() => {
    invoke<boolean>('get_can_status')
      .then(applyCanStatus)
      .catch(() => {});

    invoke<CanConnectionInfo>('get_can_connection_info')
      .then((info) => {
        if (info) applyCanInfo(info);
      })
      .catch(() => {});

    const unlistenCanStatus = listen<boolean>('can-connection-status', (event) => applyCanStatus(event.payload));
    const unlistenCanInfo = listen<CanConnectionInfo>('can-connection-info', (event) => applyCanInfo(event.payload));

    if (canChart1 && canChart2 && canChart3) {
      Plotly.newPlot(canChart1, canChartConfigs.chart1.data, canChartConfigs.chart1.layout, plotlyOptions);
      Plotly.newPlot(canChart2, canChartConfigs.chart2.data, canChartConfigs.chart2.layout, plotlyOptions);
      Plotly.newPlot(canChart3, canChartConfigs.chart3.data, canChartConfigs.chart3.layout, plotlyOptions);
      canChartsInitialized = true;
    }

    const unlistenCan = listen<CanReadData>('can-data-update', (event) => {
      if (!$canConnected) return;
      const d = event.payload;
      const nowMs = Date.now();

      hasCanData = true;

      if (d.fccu_state !== undefined) {
        fccuState = d.fccu_state;
        resolvePendingCmd('system_enable', fccuState === 2);
      }
      if (d.fccu_main_valve !== undefined) {
        fccuMainValve = d.fccu_main_valve;
        resolvePendingCmd('main_valve', fccuMainValve);
      }
      if (d.fccu_purge_valve !== undefined) {
        fccuPurgeValve = d.fccu_purge_valve;
        resolvePendingCmd('manual_purge', fccuPurgeValve);
      }
      if (d.fccu_fan_state !== undefined) {
        fccuFanState = d.fccu_fan_state;
        resolvePendingCmd('fan_on', fccuFanState);
      }

      if (d.fccu_fan_duty !== undefined) fccuFanDuty = d.fccu_fan_duty;
      if (d.fccu_version && d.fccu_version.trim().length > 0) fccuVersion = d.fccu_version;
      if (d.fccu_build_id !== undefined) fccuBuildId = d.fccu_build_id;
      if (d.fccu_vehicle_type !== undefined) fccuVehicleType = d.fccu_vehicle_type;
      if (d.fccu_fuelcell_type !== undefined) fccuFuelcellType = d.fccu_fuelcell_type;

      h2TotalVolume = d.h2_total_volume ?? 0;
      h2LpPressure = d.h2_lp_pressure ?? 0;
      h2HpPressure = d.h2_hp_pressure || d.h2_high_pressure || 0;
      h2LeakageVoltage = d.fccu_leakage_voltage || d.h2_leakage_sensor_voltage || 0;
      fcTemp = d.fc_temp ?? 0;
      ambientTemp = d.ambient_temp ?? 0;
      ambientHumidity = d.ambient_humidity ?? 0;
      ambientPressure = d.ambient_pressure ?? 0;

      fcPowerW = d.fc_w ?? 0;
      energyWh = d.energy_wh ?? 0;
      fccuDcdcVoltage = d.fccu_dcdc_voltage ?? 0;
      fccuDcdcCurrent = d.fccu_dcdc_current ?? 0;
      fccuLoadCurrent = d.fccu_load_current ?? 0;
      fccuHoCurrent1 = d.fccu_ho_current_1 ?? 0;
      fccuHoCurrent2 = d.fccu_ho_current_2 ?? 0;
      fccuHoCurrent3 = d.fccu_ho_current_3 ?? 0;
      scVoltage = d.sc_voltage ?? 0;
      scCurrent = d.sc_current ?? 0;
      mcSupplyVoltage = d.mc_supply_voltage ?? 0;
      mcSupplyCurrent = d.mc_supply_current ?? 0;
      abVoltage = d.ab_voltage ?? 0;
      abCurrent = d.ab_current ?? 0;

      speedKmh = d.speed ?? 0;
      rpmVal = d.rpm ?? 0;
      accelPedalV = d.accel_pedal_voltage ?? 0;
      brakePedalV = d.brake_pedal_voltage ?? 0;
      accelOutputV = d.accel_output_voltage ?? 0;
      brakeOutputV = d.brake_output_voltage ?? 0;
      mcuStatus = d.mcu_status ?? 0;
      vehicleType = d.vehicle_type ?? 0;
      vehicleAllowedToMove = d.vehicle_allowed_to_move ?? false;
      motorControllerEnabled = d.motor_controller_enabled ?? false;
      mainValveEnabled = d.main_valve_enabled ?? false;
      mcuActiveFaults = d.mcu_active_faults ?? [];
      ccuStatus = d.ccu_status ?? 0;
      swuCanState = d.swu_can_state ?? 0;
      lapNumber = d.lap_number ?? 0;
      lapTimeMs = d.lap_time_ms ?? 0;
      totalTimeMs = d.total_time_ms ?? 0;
      lcuFrontFault = d.lcu_front_fault ?? false;
      lcuBackFault = d.lcu_back_fault ?? false;

      if (canChartsInitialized && canChart1 && canChart2 && canChart3) {
        const xRange = buildCanXAxisRange(nowMs);
        const r1 = updateChartSeries(canChart1, chart1Samples, d.fc_output_voltage ?? 0, nowMs, xRange);
        const r2 = updateChartSeries(canChart2, chart2Samples, d.fc_output_current ?? 0, nowMs, xRange);
        const flowBatch: TimedSample[] =
          d.h2_flow_samples && d.h2_flow_samples.length > 0
            ? d.h2_flow_samples.map((s) => ({ t: s.t_ms || nowMs, v: s.flow }))
            : [{ t: nowMs, v: d.h2_flow ?? 0 }];
        const r3 = updateChartSeriesBatch(canChart3, chart3Samples, flowBatch, nowMs, xRange, CAN_FLOW_MAX_POINTS);
        chart1Samples = r1.samples;
        chart2Samples = r2.samples;
        chart3Samples = r3.samples;
        canStats = { chart1: r1.stats, chart2: r2.stats, chart3: r3.stats };
      }
    });

    return () => {
      unlistenCan.then((u) => u());
      unlistenCanStatus.then((u) => u());
      unlistenCanInfo.then((u) => u());
      clearAllPendingTimeouts();
    };
  });
</script>

<div class="can-view-wrapper" class:disabled={!$canConnected}>
  <div class="can-connection-bar" data-testid="can-connection-bar">
    <div class="can-connection-bar-left">
      <span class="can-connection-title">Połączenie CAN (PCAN-USB)</span>
      <div
        class="can-status-badge {$canConnected ? 'status-connected' : 'status-disconnected'}"
        title={$canConnectionInfo.channel_label}
      >
        <span class="status-dot"></span>
        <span class="status-text">
          {#if $canConnected}
            Połączono: {$canConnectionInfo.channel_label} · {$canConnectionInfo.baudrate_label}
          {:else if $canConnectionInfo.manual_disconnected}
            Rozłączono ręcznie
          {:else}
            Rozłączono — oczekiwanie na adapter
          {/if}
        </span>
      </div>
    </div>
    <div class="can-connection-bar-actions">
      {#if $canConnected}
        <button
          type="button"
          class="btn-disconnect-can"
          disabled={isDisconnectingPcan}
          on:click={handleDisconnectPcan}
        >
          {isDisconnectingPcan ? 'Rozłączanie...' : 'Rozłącz'}
        </button>
      {:else}
        <button
          type="button"
          class="btn-reconnect-pcan"
          disabled={isReconnectingPcan}
          on:click={handleReconnectPcan}
        >
          {isReconnectingPcan ? 'Łączenie...' : 'Połącz'}
        </button>
      {/if}
    </div>
  </div>

  {#if !$canConnected}
    <div class="can-disconnected-banner">
      <div class="can-disconnected-content">
        <span class="can-disconnected-icon">🔌</span>
        <div class="can-disconnected-text">
          <div class="can-disconnected-title">
            {$canConnectionInfo.manual_disconnected
              ? 'Połączenie z adapterem PCAN-USB zostało rozłączone ręcznie'
              : 'Brak połączenia z adapterem PCAN-USB (magistrala CAN)'}
          </div>
          <div class="can-disconnected-desc">
            {#if $canConnectionInfo.manual_disconnected}
              Zakładka CAN jest zablokowana. Kliknij „Połącz”, aby ponownie nawiązać komunikację z adapterem PCAN-USB.
            {:else}
              Zakładka CAN jest zablokowana. Trwa automatyczne nasłuchiwanie USB w tle — po podłączeniu adaptera połączenie zostanie wznowione samoczynnie.
            {/if}
          </div>
        </div>
        <div class="can-disconnected-actions">
          {#if !$canConnectionInfo.manual_disconnected}
            <div class="auto-listening-pill">
              <span class="pulsing-circle"></span>
              <span>Nasłuchiwanie USB w toku...</span>
            </div>
          {/if}
          <button
            type="button"
            class="btn-reconnect-pcan"
            disabled={isReconnectingPcan}
            on:click={handleReconnectPcan}
          >
            {isReconnectingPcan ? 'Wykrywanie...' : 'Wymuś wykrycie'}
          </button>
        </div>
      </div>
    </div>
  {/if}

  <div class="top-status-bar">
    <div class="top-status-item">
      <span class="status-lbl">Stan systemu</span>
      <span class="status-val bold" style="color: {getFccuStateColor(fccuState)}">
        {getFccuStateLabel(fccuState)}
      </span>
      <span class="status-sub">Sterownik FCCU</span>
    </div>

    <div class="top-status-item">
      <span class="status-lbl">Zawór główny</span>
      <span class="status-val bold">{fccuMainValve ? 'Otwarty' : 'Zamknięty'}</span>
      <span class="status-sub">MAIN VALVE</span>
    </div>

    <div class="top-status-item">
      <span class="status-lbl">Zawór przedmuchu</span>
      <span class="status-val bold">{fccuPurgeValve ? 'Otwarty' : 'Zamknięty'}</span>
      <span class="status-sub">PURGE VALVE</span>
    </div>

    <div class="top-status-item">
      <span class="status-lbl">Wentylator</span>
      <span class="status-val bold">{fccuFanState ? `Włączony (${fccuFanDuty}%)` : 'Wyłączony'}</span>
      <span class="status-sub">
        {fccuSettings.fanMode === 0 ? 'Tryb ręczny' : fccuSettings.fanMode === 1 ? 'Tryb liniowy' : 'Tryb PID'} · {fccuFanDuty ? fccuFanDuty * 30 : 0} obr./min
      </span>
    </div>

    <div class="top-status-item">
      <span class="status-lbl">Wersja oprogramowania</span>
      <span class="status-val bold">{fccuVersion}</span>
      <span class="status-sub">
        {#if fccuVersion === '—'}
          Brak odczytu wersji
        {:else if fccuBuildId > 0}
          Build #{fccuBuildId} · {FUELCELL_TYPE_LABELS[fccuFuelcellType] ?? 'FCCU'}
        {:else}
          Oprogramowanie FCCU
        {/if}
      </span>
    </div>
  </div>

  <div class="chart-section-card">
    <div class="chart-header-row">
      <h3 class="chart-main-title">Napięcie ogniwa (FC)</h3>
      <span class="chart-header-time">Ostatnie 60 s</span>
    </div>
    <div class="chart-card-content">
      <div class="chart-graph-box">
        {#if !hasCanData}
          <div class="chart-empty-hint">Oczekiwanie na próbki CAN</div>
        {/if}
        <div class="plotly-chart-inner" bind:this={canChart1}></div>
      </div>
      <div class="stats-column">
        {#each statRows as row}
          <div class="stat-box">
            <span class="stat-box-lbl">{row.label}</span>
            <span class="stat-box-val">{fmtVal(canStats.chart1[row.key])} V</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <div class="chart-section-card">
    <div class="chart-header-row">
      <h3 class="chart-main-title">Prąd ogniwa (FC)</h3>
      <span class="chart-header-time">Ostatnie 60 s</span>
    </div>
    <div class="chart-card-content">
      <div class="chart-graph-box">
        {#if !hasCanData}
          <div class="chart-empty-hint">Oczekiwanie na próbki CAN</div>
        {/if}
        <div class="plotly-chart-inner" bind:this={canChart2}></div>
      </div>
      <div class="stats-column">
        {#each statRows as row}
          <div class="stat-box">
            <span class="stat-box-lbl">{row.label}</span>
            <span class="stat-box-val">{fmtVal(canStats.chart2[row.key])} A</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <div class="chart-section-card" data-testid="flowmeter-chart-card">
    <div class="chart-header-row">
      <h3 class="chart-main-title">Przepływ wodoru (przepływomierz H₂)</h3>
      <div class="chart-header-meta">
        <span class="chart-meta-badge">Zużycie całkowite: {fmtVal(h2TotalVolume)} Ln</span>
        <span class="chart-header-time">Ostatnie 60 s</span>
      </div>
    </div>
    <div class="chart-card-content">
      <div class="chart-graph-box">
        {#if !hasCanData}
          <div class="chart-empty-hint">Oczekiwanie na próbki CAN</div>
        {/if}
        <div class="plotly-chart-inner" bind:this={canChart3}></div>
      </div>
      <div class="stats-column">
        {#each statRows as row}
          <div class="stat-box">
            <span class="stat-box-lbl">{row.label}</span>
            <span class="stat-box-val">{fmtVal(canStats.chart3[row.key])} Ln/min</span>
          </div>
        {/each}
      </div>
    </div>
  </div>

  <div class="section-card" data-testid="dbc-hydrogen-env-card">
    <div class="section-card-header">
      <h3 class="section-card-title">WODÓR I ŚRODOWISKO (FCCU)</h3>
    </div>
    <div class="telemetry-grid">
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Ciśnienie H₂ (LP)</span>
        <span class="telemetry-val">{fmtVal(h2LpPressure)} <span class="telemetry-unit">bar</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Ciśnienie H₂ (HP)</span>
        <span class="telemetry-val">{fmtVal(h2HpPressure, 1)} <span class="telemetry-unit">bar</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Czujnik wycieku H₂</span>
        <span class="telemetry-val">{fmtVal(h2LeakageVoltage)} <span class="telemetry-unit">V</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Zużycie całkowite H₂</span>
        <span class="telemetry-val">{fmtVal(h2TotalVolume)} <span class="telemetry-unit">Ln</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Temperatura ogniwa</span>
        <span class="telemetry-val">{fmtVal(fcTemp, 1)} <span class="telemetry-unit">°C</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Temperatura otoczenia</span>
        <span class="telemetry-val">{fmtVal(ambientTemp, 1)} <span class="telemetry-unit">°C</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Wilgotność otoczenia</span>
        <span class="telemetry-val">{fmtVal(ambientHumidity, 1)} <span class="telemetry-unit">%</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Ciśnienie atmosferyczne</span>
        <span class="telemetry-val">{fmtVal(ambientPressure, 1)} <span class="telemetry-unit">hPa</span></span>
      </div>
    </div>
  </div>

  <div class="section-card" data-testid="dbc-power-currents-card">
    <div class="section-card-header">
      <h3 class="section-card-title">ZASILANIE I PRĄDY (FCCU &amp; MCU)</h3>
    </div>
    <div class="telemetry-grid">
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Moc ogniwa (FC)</span>
        <span class="telemetry-val">{fmtVal(fcPowerW, 0)} <span class="telemetry-unit">W</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Energia (Protium)</span>
        <span class="telemetry-val">{fmtVal(energyWh, 0)} <span class="telemetry-unit">Wh</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Przetwornica DCDC</span>
        <span class="telemetry-val">{fmtVal(fccuDcdcVoltage)} <span class="telemetry-unit">V</span> · {fmtVal(fccuDcdcCurrent)} <span class="telemetry-unit">A</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Prąd obciążenia (FCCU)</span>
        <span class="telemetry-val">{fmtVal(fccuLoadCurrent)} <span class="telemetry-unit">A</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Czujniki HO-10P (1 / 2 / 3)</span>
        <span class="telemetry-val telemetry-val-sm">{fmtVal(fccuHoCurrent1, 1)} / {fmtVal(fccuHoCurrent2, 1)} / {fmtVal(fccuHoCurrent3, 1)} <span class="telemetry-unit">A</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Superkondensator (SC)</span>
        <span class="telemetry-val">{fmtVal(scVoltage)} <span class="telemetry-unit">V</span> · {fmtVal(scCurrent)} <span class="telemetry-unit">A</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Sterownik silnika (MC)</span>
        <span class="telemetry-val">{fmtVal(mcSupplyVoltage)} <span class="telemetry-unit">V</span> · {fmtVal(mcSupplyCurrent)} <span class="telemetry-unit">A</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Akumulator pomocniczy (AB)</span>
        <span class="telemetry-val">{fmtVal(abVoltage)} <span class="telemetry-unit">V</span> · {fmtVal(abCurrent)} <span class="telemetry-unit">A</span></span>
      </div>
    </div>
  </div>

  <div class="section-card" data-testid="dbc-drive-diag-card">
    <div class="section-card-header">
      <h3 class="section-card-title">NAPĘD, STEROWNIKI I DIAGNOSTYKA (MCU / SWU / CCU / LCU)</h3>
    </div>
    <div class="telemetry-grid">
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Prędkość / Obroty</span>
        <span class="telemetry-val">{fmtVal(speedKmh, 1)} <span class="telemetry-unit">km/h</span> · {fmtVal(rpmVal, 0)} <span class="telemetry-unit">rpm</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Pedały (Gaz / Hamulec)</span>
        <span class="telemetry-val">{fmtVal(accelPedalV)} <span class="telemetry-unit">V</span> / {fmtVal(brakePedalV)} <span class="telemetry-unit">V</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Wyjścia SCADE (Gaz / Ham.)</span>
        <span class="telemetry-val">{fmtVal(accelOutputV)} <span class="telemetry-unit">V</span> / {fmtVal(brakeOutputV)} <span class="telemetry-unit">V</span></span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Stan MCU / Pojazd</span>
        <span class="telemetry-val telemetry-val-sm">{MCU_STATUS_LABELS[mcuStatus] ?? mcuStatus} · {VEHICLE_TYPE_LABELS[vehicleType || fccuVehicleType] ?? '—'}</span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Zezwolenie na jazdę / MC / Zawór</span>
        <span class="telemetry-val telemetry-val-sm">
          {vehicleAllowedToMove ? 'TAK' : 'NIE'} · MC: {motorControllerEnabled ? 'ON' : 'OFF'} · ZG: {mainValveEnabled ? 'ON' : 'OFF'}
        </span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Stan CCU / SWU CAN</span>
        <span class="telemetry-val telemetry-val-sm">{CCU_STATUS_LABELS[ccuStatus] ?? ccuStatus} · {SWU_CAN_STATE_LABELS[swuCanState] ?? swuCanState}</span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Okrążenie (SWU)</span>
        <span class="telemetry-val telemetry-val-sm">#{lapNumber} · {formatDurationMs(lapTimeMs)} / {formatDurationMs(totalTimeMs)}</span>
      </div>
      <div class="telemetry-tile">
        <span class="telemetry-lbl">Oświetlenie LCU (Przód / Tył)</span>
        <span class="telemetry-val telemetry-val-sm">
          <span class:diag-err={lcuFrontFault} class:diag-ok={!lcuFrontFault}>{lcuFrontFault ? 'BŁĄD' : 'OK'}</span>
          /
          <span class:diag-err={lcuBackFault} class:diag-ok={!lcuBackFault}>{lcuBackFault ? 'BŁĄD' : 'OK'}</span>
        </span>
      </div>
    </div>

    <div class="faults-bar">
      <span class="faults-lbl">Błędy MCU:</span>
      {#if mcuActiveFaults.length === 0}
        <span class="fault-badge ok">Brak aktywnych błędów</span>
      {:else}
        <div class="fault-badges-list">
          {#each mcuActiveFaults as fault}
            <span class="fault-badge err">{fault}</span>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">KOMENDY STEROWNIKA FCCU</h3>
    </div>
    <div class="cmd-grid-2x2">
      {#each cmdTiles as tile}
        <div class="cmd-tile" class:cmd-tile-fan={tile.key === 'fan_on'}>
          <div class="cmd-tile-main">
            <div class="cmd-tile-info">
              <span class="cmd-tile-name">{tile.name}</span>
              <span class="cmd-tile-state">Stan: {tile.stateText}</span>
            </div>
            <button
              class="action-btn {tile.btnClass}"
              class:is-pending={pendingCmds[tile.key]}
              disabled={pendingCmds[tile.key]}
              on:click={() => toggleFccuCmd(tile.key)}
            >
              {#if pendingCmds[tile.key]}
                <span class="btn-spinner"></span>
                <span>{expectedStates[tile.key] ? tile.pendingOn : tile.pendingOff}</span>
              {:else}
                {tile.active ? tile.labelOn : tile.labelOff}
              {/if}
            </button>
          </div>

          {#if tile.key === 'fan_on'}
            <div class="fan-duty-row">
              <label for="fan-duty-number" class="fan-duty-lbl">Wysterowanie (%)</label>
              <input
                type="range"
                class="fan-duty-slider"
                min="0"
                max="100"
                step="1"
                bind:value={fanDutyCmd}
                aria-label="Wypełnienie wentylatora w procentach"
              />
              <div class="input-unit-wrapper fan-duty-number-wrap">
                <input
                  id="fan-duty-number"
                  type="number"
                  class="setting-form-input fan-duty-number"
                  min="0"
                  max="100"
                  bind:value={fanDutyCmd}
                  on:change={() => (fanDutyCmd = clampFanDuty(fanDutyCmd))}
                  on:keydown={(e) => e.key === 'Enter' && sendFanDuty()}
                />
                <span class="input-unit-badge">%</span>
              </div>
              <button
                type="button"
                class="action-btn btn-blue fan-duty-btn"
                disabled={isSendingFanDuty}
                on:click={sendFanDuty}
              >
                {isSendingFanDuty ? 'Wysyłanie...' : 'Ustaw'}
              </button>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">USTAWIENIA STEROWNIKA FCCU</h3>
    </div>

    <div class="settings-content-layout">
      <div class="settings-form-column">
        <div class="setting-form-row">
          <label for="fan-mode-select" class="setting-form-label">Tryb wentylatora</label>
          <select id="fan-mode-select" class="setting-form-select" bind:value={fccuSettings.fanMode}>
            <option value={0}>Ręczny</option>
            <option value={1}>Liniowy temp.</option>
            <option value={2}>Regulator PID</option>
          </select>
          <span class="setting-form-helper">Wybór sposobu sterowania</span>
        </div>

        <div class="setting-form-row">
          <label for="purge-interval-input" class="setting-form-label">Odstęp między przedmuchami</label>
          <div class="input-unit-wrapper">
            <input id="purge-interval-input" type="number" class="setting-form-input" bind:value={fccuSettings.purgeInterval} min="0" max="16383" />
            <span class="input-unit-badge">s</span>
          </div>
          <span class="setting-form-helper">Czas między cyklami purge</span>
        </div>

        {#each pidFields as field}
          <div class="setting-form-row" class:disabled={!isPidEnabled}>
            <label for={field.id} class="setting-form-label">{field.label}</label>
            <input
              id={field.id}
              type="number"
              class="setting-form-input"
              bind:value={fccuSettings[field.key]}
              min="0"
              max="1000"
              disabled={!isPidEnabled}
            />
            <span class="setting-form-helper">{field.helper}</span>
          </div>
        {/each}
      </div>

      <div class="settings-readout-column">
        <div class="fan-readout-panel">
          <h4 class="fan-readout-panel-title">BIEŻĄCY ODCZYT WENTYLATORA</h4>
          <div class="fan-readout-item">
            <span class="fan-readout-key">Stan</span>
            <span class="fan-readout-val bold">{fccuFanState ? 'Włączony' : 'Wyłączony'}</span>
          </div>
          <div class="fan-readout-item">
            <span class="fan-readout-key">Tryb</span>
            <span class="fan-readout-val">
              {fccuSettings.fanMode === 0 ? 'Ręczny' : fccuSettings.fanMode === 1 ? 'Liniowy' : 'Regulator PID'}
            </span>
          </div>
          <div class="fan-readout-item">
            <span class="fan-readout-key">Wypełnienie</span>
            <span class="fan-readout-val">{fccuFanDuty} %</span>
          </div>
          <div class="fan-readout-item">
            <span class="fan-readout-key">Prędkość</span>
            <span class="fan-readout-val">{fccuFanDuty ? fccuFanDuty * 30 : 0} obr./min</span>
          </div>
        </div>
        <button class="blue-submit-btn" disabled={isSendingSettings} on:click={sendFccuSettings}>
          {isSendingSettings ? 'Wysyłanie...' : 'Wyślij ustawienia'}
        </button>
      </div>
    </div>
  </div>

  <div class="section-card">
    <div class="section-card-header">
      <h3 class="section-card-title">STEROWANIE OŚWIETLENIEM</h3>
    </div>

    <div class="lighting-grid-layout">
      {#each lightingColumns as col}
        <div class="lighting-column">
          {#each col as item}
            <div class="light-switch-row">
              <label class="switch-container">
                <input type="checkbox" bind:checked={lighting[item.key]} />
                <span class="switch-track"></span>
                <span class="switch-label-text">{item.label}</span>
              </label>
              <span class="switch-status-text">{lighting[item.key] ? 'Włączone' : 'Wyłączone'}</span>
            </div>
          {/each}
        </div>
      {/each}
    </div>

    <div class="lighting-action-row">
      <button class="blue-submit-btn" disabled={isSendingLighting} on:click={sendLightingCmd}>
        {isSendingLighting ? 'Wysyłanie...' : 'Wyślij oświetlenie'}
      </button>
    </div>
  </div>

  {#if toastVisible}
    <div class="toast-container" transition:fly={{ y: 24, duration: 250 }}>
      <div class="toast-card {toastType}">
        <span class="toast-icon">
          {#if toastType === 'error'}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <circle cx="12" cy="12" r="10"></circle>
              <line x1="12" y1="8" x2="12" y2="12"></line>
              <line x1="12" y1="16" x2="12.01" y2="16"></line>
            </svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12"></polyline>
            </svg>
          {/if}
        </span>
        <div class="toast-body">
          <span class="toast-title">{toastType === 'error' ? 'Błąd CAN' : 'Wysłano ramkę CAN'}</span>
          <span class="toast-message">{toastMessage}</span>
        </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .can-view-wrapper {
    display: flex;
    flex-direction: column;
    gap: 16px;
    position: relative;
    transition: opacity 0.3s ease, filter 0.3s ease;
  }

  .can-view-wrapper.disabled > *:not(.can-disconnected-banner):not(.can-connection-bar):not(.toast-container) {
    opacity: 0.32;
    filter: grayscale(0.85);
    pointer-events: none;
    user-select: none;
  }

  .can-connection-bar {
    pointer-events: auto;
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 12px 18px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .can-connection-bar-left {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }

  .can-connection-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #e4e4e7;
  }

  .can-status-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 9999px;
    font-size: 0.78rem;
    font-weight: 500;
    background-color: #27272a;
    color: #a1a1aa;
  }

  .can-status-badge.status-connected {
    background-color: rgba(34, 197, 94, 0.15);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.3);
  }

  .can-status-badge.status-disconnected {
    background-color: rgba(239, 68, 68, 0.12);
    color: #f87171;
    border: 1px solid rgba(239, 68, 68, 0.25);
  }

  .can-status-badge .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background-color: currentColor;
    flex-shrink: 0;
  }

  .can-connection-bar-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .btn-disconnect-can {
    background: rgba(239, 68, 68, 0.14);
    border: 1px solid rgba(239, 68, 68, 0.45);
    color: #fca5a5;
    font-weight: 600;
    font-size: 0.82rem;
    padding: 7px 14px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .btn-disconnect-can:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.26);
    color: #ffffff;
    border-color: #ef4444;
  }

  .btn-disconnect-can:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .can-disconnected-banner {
    pointer-events: auto;
    background: rgba(24, 24, 27, 0.95);
    border: 1px solid rgba(239, 68, 68, 0.4);
    border-radius: 10px;
    padding: 14px 20px;
    backdrop-filter: blur(6px);
    box-shadow: 0 4px 18px rgba(0, 0, 0, 0.5);
  }

  .can-disconnected-content {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .can-disconnected-icon {
    font-size: 1.5rem;
  }

  .can-disconnected-text {
    flex: 1;
    min-width: 200px;
  }

  .can-disconnected-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #f87171;
  }

  .can-disconnected-desc {
    font-size: 0.8rem;
    color: #a1a1aa;
    margin-top: 3px;
  }

  .btn-reconnect-pcan {
    background: linear-gradient(135deg, #2563eb, #1d4ed8);
    border: 1px solid #3b82f6;
    color: #ffffff;
    font-weight: 600;
    font-size: 0.82rem;
    padding: 8px 16px;
    border-radius: 6px;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .btn-reconnect-pcan:hover:not(:disabled) {
    background: linear-gradient(135deg, #3b82f6, #2563eb);
    box-shadow: 0 0 10px rgba(59, 130, 246, 0.4);
  }

  .btn-reconnect-pcan:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .can-disconnected-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .auto-listening-pill {
    display: flex;
    align-items: center;
    gap: 8px;
    background: rgba(39, 39, 42, 0.85);
    border: 1px solid #3f3f46;
    padding: 6px 12px;
    border-radius: 9999px;
    font-size: 0.78rem;
    color: #cbd5e1;
    white-space: nowrap;
  }

  .pulsing-circle {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background-color: #38bdf8;
    box-shadow: 0 0 0 rgba(56, 189, 248, 0.5);
    animation: pulse-glow 1.5s infinite;
  }

  @keyframes pulse-glow {
    0% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(56, 189, 248, 0.7);
    }
    70% {
      transform: scale(1);
      box-shadow: 0 0 0 6px rgba(56, 189, 248, 0);
    }
    100% {
      transform: scale(0.95);
      box-shadow: 0 0 0 0 rgba(56, 189, 248, 0);
    }
  }

  .top-status-bar {
    display: grid;
    grid-template-columns: repeat(5, 1fr);
    gap: 12px;
  }

  .top-status-item {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .status-lbl {
    font-size: 0.72rem;
    color: #a1a1aa;
  }

  .status-val {
    font-size: 1.15rem;
    color: #ffffff;
    font-weight: 500;
  }

  .status-val.bold {
    font-weight: 700;
  }

  .status-sub {
    font-size: 0.68rem;
    color: #71717a;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .chart-section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .chart-header-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .chart-main-title {
    font-size: 0.95rem;
    font-weight: 600;
    color: #ffffff;
    margin: 0;
  }

  .chart-header-time {
    font-size: 0.75rem;
    color: #71717a;
  }

  .chart-header-meta {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .chart-meta-badge {
    font-size: 0.74rem;
    font-weight: 600;
    color: #22d3ee;
    background: rgba(6, 182, 212, 0.12);
    border: 1px solid rgba(6, 182, 212, 0.3);
    padding: 2px 9px;
    border-radius: 9999px;
  }

  .telemetry-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 10px;
  }

  .telemetry-tile {
    background-color: #121215;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .telemetry-lbl {
    font-size: 0.7rem;
    color: #a1a1aa;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .telemetry-val {
    font-size: 0.96rem;
    font-weight: 700;
    color: #ffffff;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .telemetry-val-sm {
    font-size: 0.82rem;
    font-weight: 600;
  }

  .telemetry-unit {
    font-size: 0.72rem;
    font-weight: 400;
    color: #71717a;
  }

  .diag-ok {
    color: #4ade80;
  }

  .diag-err {
    color: #f87171;
  }

  .faults-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding-top: 8px;
    border-top: 1px solid #27272a;
  }

  .faults-lbl {
    font-size: 0.75rem;
    font-weight: 600;
    color: #a1a1aa;
  }

  .fault-badges-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .fault-badge {
    font-size: 0.72rem;
    font-weight: 600;
    padding: 3px 9px;
    border-radius: 9999px;
  }

  .fault-badge.ok {
    background: rgba(34, 197, 94, 0.12);
    border: 1px solid rgba(34, 197, 94, 0.3);
    color: #4ade80;
  }

  .fault-badge.err {
    background: rgba(239, 68, 68, 0.14);
    border: 1px solid rgba(239, 68, 68, 0.4);
    color: #f87171;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }

  @media (max-width: 1100px) {
    .telemetry-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  .chart-card-content {
    display: flex;
    gap: 16px;
    align-items: stretch;
  }

  .chart-graph-box {
    flex: 1;
    height: 150px;
    position: relative;
    min-width: 0;
  }

  .chart-empty-hint {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #52525b;
    font-size: 0.85rem;
    pointer-events: none;
    z-index: 2;
  }

  .plotly-chart-inner {
    width: 100%;
    height: 100%;
  }

  .stats-column {
    width: 140px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    justify-content: center;
    flex-shrink: 0;
  }

  .stat-box {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .stat-box-lbl {
    font-size: 0.68rem;
    color: #71717a;
  }

  .stat-box-val {
    font-size: 1.05rem;
    font-weight: 700;
    color: #ffffff;
  }

  .section-card {
    background-color: #18181b;
    border: 1px solid #27272a;
    border-radius: 12px;
    padding: 18px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-card-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .section-card-title {
    font-size: 0.85rem;
    font-weight: 700;
    color: #e4e4e7;
    letter-spacing: 0.05em;
    margin: 0;
  }

  .cmd-grid-2x2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 20px;
  }

  .cmd-tile {
    background-color: #121215;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 10px 14px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 10px;
  }

  .cmd-tile-main {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }

  .fan-duty-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding-top: 10px;
    border-top: 1px solid #27272a;
  }

  .fan-duty-lbl {
    font-size: 0.75rem;
    color: #a1a1aa;
    white-space: nowrap;
  }

  .fan-duty-slider {
    flex: 1;
    min-width: 60px;
    accent-color: #3b82f6;
    cursor: pointer;
  }

  .fan-duty-number-wrap {
    width: 78px;
    flex-shrink: 0;
  }

  .fan-duty-number {
    padding: 6px 24px 6px 8px;
    font-size: 0.82rem;
  }

  .fan-duty-btn {
    border: none;
    padding: 7px 14px;
  }

  .cmd-tile-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .cmd-tile-name {
    font-size: 0.85rem;
    font-weight: 600;
    color: #ffffff;
  }

  .cmd-tile-state {
    font-size: 0.75rem;
    color: #71717a;
  }

  .action-btn {
    border-radius: 6px;
    padding: 8px 16px;
    font-size: 0.82rem;
    cursor: pointer;
    transition: all 0.2s ease;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }

  .action-btn:disabled,
  .action-btn.is-pending {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.6);
    pointer-events: none;
  }

  .btn-spinner {
    width: 12px;
    height: 12px;
    border: 2px solid rgba(255, 255, 255, 0.3);
    border-top-color: #ffffff;
    border-radius: 50%;
    animation: spin-btn 0.7s linear infinite;
    display: inline-block;
    flex-shrink: 0;
  }

  @keyframes spin-btn {
    to {
      transform: rotate(360deg);
    }
  }

  .btn-blue {
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 600;
  }
  .btn-blue:hover { background-color: #2563eb; }

  .btn-grey {
    background-color: #27272a;
    color: #e4e4e7;
    border: 1px solid #52525b;
    font-weight: 500;
  }
  .btn-grey:hover { background-color: #3f3f46; border-color: #71717a; }

  .settings-content-layout {
    display: flex;
    gap: 24px;
    align-items: stretch;
  }

  .settings-form-column {
    flex: 1.3;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .setting-form-row {
    display: flex;
    align-items: center;
    gap: 14px;
    transition: opacity 0.25s ease;
  }

  .setting-form-row.disabled {
    opacity: 0.42;
    cursor: not-allowed;
  }

  .setting-form-row.disabled .setting-form-label {
    color: #71717a;
    cursor: not-allowed;
  }

  .setting-form-row.disabled .setting-form-helper {
    color: #52525b;
  }

  .setting-form-label {
    width: 190px;
    font-size: 0.82rem;
    color: #d4d4d8;
    flex-shrink: 0;
  }

  .setting-form-select,
  .setting-form-input {
    background-color: #27272a;
    border: 1px solid #3f3f46;
    color: #ffffff;
    border-radius: 6px;
    padding: 7px 12px;
    font-size: 0.85rem;
    width: 120px;
    outline: none;
    transition: background-color 0.25s ease, border-color 0.25s ease, color 0.25s ease;
  }
  .setting-form-select:focus,
  .setting-form-input:focus {
    border-color: #3b82f6;
  }

  .setting-form-input:disabled {
    background-color: #18181b;
    border-color: #27272a;
    color: #52525b;
    cursor: not-allowed;
    box-shadow: none;
    user-select: none;
  }

  .input-unit-wrapper {
    position: relative;
    display: inline-flex;
    align-items: center;
    width: 120px;
  }

  .input-unit-wrapper .setting-form-input {
    width: 100%;
    padding-right: 28px;
  }

  .input-unit-badge {
    position: absolute;
    right: 10px;
    font-size: 0.75rem;
    color: #71717a;
    pointer-events: none;
  }

  .setting-form-helper {
    font-size: 0.75rem;
    color: #71717a;
  }

  .settings-readout-column {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 10px;
    justify-content: space-between;
  }

  .fan-readout-panel {
    background-color: #121215;
    border: 1px solid #27272a;
    border-radius: 8px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .fan-readout-panel-title {
    font-size: 0.72rem;
    font-weight: 700;
    color: #71717a;
    letter-spacing: 0.05em;
    margin: 0 0 4px 0;
  }

  .fan-readout-item {
    display: flex;
    justify-content: space-between;
    font-size: 0.85rem;
  }

  .fan-readout-key { color: #a1a1aa; }
  .fan-readout-val { color: #e4e4e7; }
  .fan-readout-val.bold { font-weight: 700; color: #ffffff; }

  .blue-submit-btn {
    align-self: flex-end;
    background-color: #3b82f6;
    color: #ffffff;
    font-weight: 600;
    border: none;
    border-radius: 6px;
    padding: 10px 22px;
    font-size: 0.85rem;
    cursor: pointer;
    transition: background-color 0.2s;
  }
  .blue-submit-btn:hover { background-color: #2563eb; }
  .blue-submit-btn:disabled {
    opacity: 0.45;
    cursor: not-allowed;
    filter: grayscale(0.5);
    pointer-events: none;
  }

  .lighting-grid-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px 36px;
  }

  .lighting-column {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .light-switch-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 4px 0;
  }

  .switch-container {
    display: flex;
    align-items: center;
    gap: 12px;
    cursor: pointer;
    user-select: none;
  }

  .switch-container input {
    display: none;
  }

  .switch-track {
    width: 36px;
    height: 20px;
    background-color: #3f3f46;
    border-radius: 10px;
    position: relative;
    transition: background-color 0.2s ease;
  }

  .switch-track::after {
    content: '';
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    background-color: #ffffff;
    border-radius: 50%;
    transition: transform 0.2s ease;
  }

  .switch-container input:checked + .switch-track {
    background-color: #3b82f6;
  }

  .switch-container input:checked + .switch-track::after {
    transform: translateX(16px);
  }

  .switch-label-text {
    font-size: 0.85rem;
    color: #e4e4e7;
  }

  .switch-status-text {
    font-size: 0.78rem;
    color: #71717a;
  }

  .lighting-action-row {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    border-top: 1px solid #27272a;
    padding-top: 14px;
    margin-top: 4px;
  }

  .toast-container {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 9999;
    pointer-events: none;
  }

  .toast-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 18px;
    border-radius: 8px;
    box-shadow: 0 10px 25px -5px rgba(0, 0, 0, 0.6), 0 8px 10px -6px rgba(0, 0, 0, 0.4);
    min-width: 260px;
    max-width: 450px;
    backdrop-filter: blur(8px);
    pointer-events: auto;
  }

  .toast-card.success {
    background-color: #0f231c;
    border: 1px solid #10b981;
    color: #ecfdf5;
  }

  .toast-card.error {
    background-color: #271216;
    border: 1px solid #ef4444;
    color: #fef2f2;
  }

  .toast-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .toast-card.success .toast-icon {
    color: #10b981;
  }

  .toast-card.error .toast-icon {
    color: #ef4444;
  }

  .toast-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toast-title {
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #a7f3d0;
  }

  .toast-card.error .toast-title {
    color: #fca5a5;
  }

  .toast-message {
    font-size: 0.88rem;
    font-weight: 500;
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
  }
</style>
