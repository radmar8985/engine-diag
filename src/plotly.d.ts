declare module 'plotly.js-dist-min' {
  export type PlotType =
    | 'scatter'
    | 'bar'
    | 'pie'
    | 'histogram'
    | 'box'
    | 'heatmap'
    | 'contour'
    | string;

  export type PlotMode =
    | 'lines'
    | 'markers'
    | 'lines+markers'
    | 'none'
    | string;

  export interface ScatterLine {
    color?: string;
    width?: number;
    dash?: 'solid' | 'dot' | 'dash' | 'longdash' | 'dashdot' | 'longdashdot' | string;
    shape?: 'linear' | 'spline' | 'hv' | 'vh' | 'hvh' | 'vhv' | string;
  }

  export interface PlotData {
    x?: (string | number | Date)[];
    y?: (number | null | undefined)[];
    type?: PlotType;
    mode?: PlotMode;
    name?: string;
    line?: ScatterLine;
    marker?: Record<string, unknown>;
    text?: string | string[];
    hoverinfo?: string;
    showlegend?: boolean;
    [key: string]: unknown;
  }

  export interface FontConfig {
    family?: string;
    size?: number;
    color?: string;
  }

  export interface AxisConfig {
    title?: string | { text: string; font?: FontConfig };
    gridcolor?: string;
    zeroline?: boolean;
    tickfont?: FontConfig;
    tickmode?: 'auto' | 'linear' | 'array';
    dtick?: number | string;
    range?: [string | number | Date, string | number | Date];
    autorange?: boolean | 'reversed';
    showgrid?: boolean;
    [key: string]: unknown;
  }

  export interface MarginConfig {
    t?: number;
    r?: number;
    b?: number;
    l?: number;
    pad?: number;
  }

  export interface LegendConfig {
    orientation?: 'h' | 'v';
    x?: number;
    y?: number;
    xanchor?: 'auto' | 'left' | 'center' | 'right';
    yanchor?: 'auto' | 'top' | 'middle' | 'bottom';
    font?: FontConfig;
  }

  export interface PlotLayout {
    title?: string | { text: string; font?: FontConfig };
    margin?: MarginConfig;
    paper_bgcolor?: string;
    plot_bgcolor?: string;
    font?: FontConfig;
    xaxis?: AxisConfig;
    yaxis?: AxisConfig;
    showlegend?: boolean;
    legend?: LegendConfig;
    hovermode?: 'x' | 'y' | 'closest' | false | 'x unified' | 'y unified';
    responsive?: boolean;
    autosize?: boolean;
    width?: number;
    height?: number;
    [key: string]: unknown;
  }

  export interface PlotlyConfig {
    responsive?: boolean;
    displayModeBar?: boolean | 'hover';
    displaylogo?: boolean;
    modeBarButtonsToRemove?: string[];
    [key: string]: unknown;
  }

  export interface PlotlyExtendData {
    x?: (string | number | Date)[][];
    y?: (number | null | undefined)[][];
    [key: string]: unknown;
  }

  export type Root = string | HTMLElement;

  export function newPlot(
    root: Root,
    data: PlotData[],
    layout?: Partial<PlotLayout>,
    config?: Partial<PlotlyConfig>
  ): Promise<HTMLElement>;

  export function extendTraces(
    root: Root,
    update: PlotlyExtendData,
    indices: number[],
    maxPoints?: number
  ): Promise<HTMLElement>;

  export function relayout(
    root: Root,
    layout: Partial<PlotLayout>
  ): Promise<HTMLElement>;

  export function update(
    root: Root,
    traceUpdate: PlotlyExtendData,
    layoutUpdate: Partial<PlotLayout>,
    traceIndices?: number | number[]
  ): Promise<HTMLElement>;

  export function restyle(
    root: Root,
    aobj: Record<string, unknown>,
    traces?: number | number[]
  ): Promise<HTMLElement>;

  export function purge(root: Root): void;

  export interface PlotlyStatic {
    newPlot: typeof newPlot;
    extendTraces: typeof extendTraces;
    relayout: typeof relayout;
    update: typeof update;
    restyle: typeof restyle;
    purge: typeof purge;
  }

  const Plotly: PlotlyStatic;
  export default Plotly;
}
