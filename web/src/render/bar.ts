/**
 * The bar of a part of the genetics card (Grok M2): from the middle on a fixed scale of ±2,
 * positive to the right, negative to the left, held at the ends beyond (the number beside it
 * stays true). `left` and `width` are percent of the bar's box.
 */
export function bar(value: number): { left: number; width: number } {
  const held = Math.min(Math.max(value, -2), 2);
  const width = (Math.abs(held) / 2) * 50;
  return { left: held < 0 ? 50 - width : 50, width };
}
