import { describe, expect, it } from "vitest";
import { readRequest } from "./modTool";

describe("Skin Studio host calls", () => {
  it("accepts cross-game reads only through the existing roots", () => {
    expect(
      readRequest({
        openrac: 1,
        id: 7,
        call: "targetList",
        args: { game: "rac3", version: "pal", root: "levels" },
      }),
    ).toEqual({ id: 7, call: "targetList", game: "rac3", version: "pal", root: "levels" });

    const bad = readRequest({
      openrac: 1,
      id: 8,
      call: "targetRead",
      args: { game: "../rac3", version: "pal", root: "levels", path: "level_00/manifest.json" },
    });
    expect(bad && "error" in bad).toBe(true);
  });

  it("bounds direct pack installation and never accepts a path", () => {
    const bytes = new ArrayBuffer(32);
    expect(
      readRequest({
        openrac: 1,
        id: 9,
        call: "applyPack",
        args: { game: "rac1", version: "pal", name: "my-skin", bytes },
      }),
    ).toEqual({ id: 9, call: "applyPack", game: "rac1", version: "pal", name: "my-skin", bytes });

    const bad = readRequest({
      openrac: 1,
      id: 10,
      call: "applyPack",
      args: { game: "rac1", version: "pal", name: "my-skin", path: "/tmp/mod.zip" },
    });
    expect(bad && "error" in bad).toBe(true);
  });
});
