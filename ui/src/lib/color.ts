import type { Color } from "./model";
export function toHex(color: Color, linear = true): string {
  return (
    "#" +
    color
      .slice(0, 3)
      .map((v) => {
        const s = linear ? (v <= 0.0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - 0.055) : v;
        return Math.round(Math.max(0, Math.min(1, s)) * 255)
          .toString(16)
          .padStart(2, "0");
      })
      .join("")
  );
}
export function fromHex(hex: string, alpha = 1, linear = true): Color {
  const s = hex.replace("#", "");
  const values = [0, 2, 4]
    .map((i) => parseInt(s.slice(i, i + 2), 16) / 255)
    .map((v) => (linear ? (v <= 0.04045 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4)) : v));
  return [values[0], values[1], values[2], alpha];
}
