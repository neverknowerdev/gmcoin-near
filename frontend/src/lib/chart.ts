import { compact, gmToNumber } from "./format";
import type { Epoch } from "./types";

export interface Bar {
  epoch: number;
  x: number;
  y: number;
  w: number;
  h: number;
  value: number;
}
export interface ChartGeometry {
  bars: Bar[];
  line: { x: number; y: number; epoch: number; value: number }[];
  ticks: { y: number; label: string }[];
  labels: { x: number; label: string }[];
  yMax: number;
}

/** Nice axis max: 1, 2, 5 x 10^n at or above v. */
export function niceMax(v: number): number {
  if (!(v > 0)) return 1;
  const p = 10 ** Math.floor(Math.log10(v));
  for (const m of [1, 2, 5, 10]) if (m * p >= v) return m * p;
  return 10 * p;
}

export interface Box {
  width: number;
  height: number;
  left: number;
  right: number;
  top: number;
  bottom: number;
}

/** Bars = weekly budget, line = minted, one slot per epoch. */
export function chartGeometry(epochs: Epoch[], box: Box): ChartGeometry {
  const innerW = box.width - box.left - box.right;
  const innerH = box.height - box.top - box.bottom;
  const budgets = epochs.map((e) => gmToNumber(e.budget));
  const minted = epochs.map((e) => gmToNumber(e.minted));
  const yMax = niceMax(Math.max(0, ...budgets, ...minted));
  const slot = epochs.length ? innerW / epochs.length : innerW;
  const bw = Math.min(56, slot * 0.6);
  const yOf = (v: number) => box.top + innerH - (v / yMax) * innerH;
  const xc = (i: number) => box.left + slot * i + slot / 2;
  const bars = budgets.map((v, i) => ({
    epoch: epochs[i].index,
    x: xc(i) - bw / 2,
    y: yOf(v),
    w: bw,
    h: box.top + innerH - yOf(v),
    value: v,
  }));
  const line = minted.map((v, i) => ({ x: xc(i), y: yOf(v), epoch: epochs[i].index, value: v }));
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => ({ y: yOf(yMax * f), label: compact(yMax * f) }));
  const labels = epochs.map((e, i) => ({ x: xc(i), label: String(e.index) }));
  return { bars, line, ticks, labels, yMax };
}
