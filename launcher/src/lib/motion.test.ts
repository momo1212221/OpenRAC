import { afterEach, expect, it, vi } from "vitest";

vi.mock("svelte", () => ({ tick: () => Promise.resolve() }));

afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});

it("removes a stuck snapshot transition and keeps subsequent navigation live", async () => {
  const skipTransition = vi.fn();
  const transition = { finished: new Promise<void>(() => undefined), skipTransition };
  const startViewTransition = vi.fn(() => transition);
  const dataset: Record<string, string> = { motion: "slides" };
  vi.stubGlobal("document", {
    documentElement: { dataset },
    addEventListener: vi.fn(),
    startViewTransition,
  });
  const motion = await import("./motion");
  motion.slide(vi.fn(), "forward");
  motion.stopSlides();
  expect(skipTransition).toHaveBeenCalledOnce();
  expect(dataset.motion).toBeUndefined();
  const change = vi.fn();
  motion.slide(change, "back");
  expect(change).toHaveBeenCalledOnce();
  expect(startViewTransition).toHaveBeenCalledOnce();
});
