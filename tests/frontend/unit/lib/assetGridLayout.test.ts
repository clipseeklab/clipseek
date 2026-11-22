import { describe, expect, it } from "vitest";
import { GRID_GAP, assetAspectRatio, layoutAssets, visibleCells } from "@/lib/assetGridLayout";
import { buildAsset } from "@test/fixtures/assets";

const assets = Array.from({ length: 10_000 }, (_, id) =>
  buildAsset({
    id,
    width: 100 + (id % 700),
    height: 100 + (id % 500),
  }),
);

describe("asset grid layout", () => {
  it("reserves one metadata row regardless of tag count so consecutive cards do not overlap", () => {
    const plain = buildAsset({ width: 100, height: 100 });
    const tagged = buildAsset({ width: 100, height: 100, tags: ["one", "two", "three"] });
    const { columns } = layoutAssets([plain, tagged, plain], 202, 1);
    expect(columns[0].map((cell) => cell.height)).toEqual([254, 254, 254]);
    expect(columns[0][1].top).toBe(254 + GRID_GAP);
    expect(columns[0][2].top).toBe(254 * 2 + GRID_GAP * 2);
  });

  it("uses the rendered-card fallback for invalid dimensions", () => {
    expect(assetAspectRatio({ kind: "image", width: 0, height: 100 })).toBe(4 / 3);
    expect(assetAspectRatio({ kind: "video", width: -1, height: 100 })).toBe(16 / 9);
  });

  it("retains column order and positions when another page arrives", () => {
    for (const count of [2, 4, 8]) {
      const first = layoutAssets(assets.slice(0, 40), 1_100, count);
      const appended = layoutAssets(assets.slice(0, 80), 1_100, count);
      first.columns.forEach((column, index) => {
        expect(column).toEqual(appended.columns[index].slice(0, column.length));
        column.forEach((cell, row) => {
          expect(cell.index).toBe(row * count + index);
          if (row > 0) {
            expect(cell.top).toBe(column[row - 1].top + column[row - 1].height + GRID_GAP);
          }
        });
      });
    }
  });

  it("covers fast viewport jumps with a bounded rendered-card count", () => {
    for (const width of [640, 1_200, 1_800]) {
      const layout = layoutAssets(assets, width, 4);
      for (const top of [0, 5_000, layout.height / 2, layout.height - 800]) {
        let count = 0;
        layout.columns.forEach((column) => {
          const actual = visibleCells(column, top - 1_000, top + 1_800);
          const expected = column.filter(
            (cell) => cell.top + cell.height >= top - 1_000 && cell.top <= top + 1_800,
          );
          expect(actual).toEqual(expected);
          count += actual.length;
        });
        expect(count).toBeLessThan(150);
      }
    }
  });

  it("handles empty lists and out-of-range windows", () => {
    expect(layoutAssets([], 1_000, 4).height).toBe(0);
    expect(visibleCells([], 0, 1_000)).toEqual([]);
    expect(visibleCells([{ index: 0, top: 0, height: 10 }], 11, 20)).toEqual([]);
  });
});
