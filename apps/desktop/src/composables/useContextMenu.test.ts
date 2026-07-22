import { describe, it, expect, afterEach, beforeEach } from "vitest";
import { nextTick } from "vue";
import { useContextMenu } from "./useContextMenu";

/*
 * useContextMenu owns the shared right-click-menu positioning. The behaviour
 * that matters: a menu opened too close to the right or bottom edge flips
 * inward (minus a 4px margin) so it is never clipped, and a menu that already
 * fits is left alone. The version-history menu sits at the window's right edge,
 * so this is what keeps it fully visible.
 */

function makeEl(width: number, height: number): HTMLElement {
  return {
    getBoundingClientRect: () => ({
      width,
      height,
      x: 0,
      y: 0,
      top: 0,
      left: 0,
      right: width,
      bottom: height,
      toJSON: () => ({}),
    }),
  } as unknown as HTMLElement;
}

function setViewport(width: number, height: number) {
  Object.defineProperty(window, "innerWidth", {
    value: width,
    writable: true,
    configurable: true,
  });
  Object.defineProperty(window, "innerHeight", {
    value: height,
    writable: true,
    configurable: true,
  });
}

describe("useContextMenu", () => {
  const originalWidth = window.innerWidth;
  const originalHeight = window.innerHeight;

  beforeEach(() => setViewport(800, 600));
  afterEach(() => {
    setViewport(originalWidth, originalHeight);
  });

  it("opens at the cursor position and is visible", () => {
    const menu = useContextMenu();
    menu.openAt({ clientX: 120, clientY: 80 } as MouseEvent);
    expect(menu.open.value).toBe(true);
    expect(menu.pos.value).toEqual({ x: 120, y: 80 });
  });

  it("close hides the menu", () => {
    const menu = useContextMenu();
    menu.openAt({ clientX: 10, clientY: 10 } as MouseEvent);
    menu.close();
    expect(menu.open.value).toBe(false);
  });

  it("clamps a menu overflowing the right edge leftward", () => {
    const menu = useContextMenu();
    menu.menuRef.value = makeEl(160, 200);
    menu.pos.value = { x: 700, y: 100 }; // 700 + 160 = 860 > 800
    menu.clamp();
    expect(menu.pos.value.x).toBe(800 - 160 - 4); // 636
    expect(menu.pos.value.y).toBe(100);
  });

  it("clamps a menu overflowing the bottom edge upward", () => {
    const menu = useContextMenu();
    menu.menuRef.value = makeEl(160, 200);
    menu.pos.value = { x: 100, y: 500 }; // 500 + 200 = 700 > 600
    menu.clamp();
    expect(menu.pos.value.y).toBe(600 - 200 - 4); // 396
    expect(menu.pos.value.x).toBe(100);
  });

  it("does not move a menu that already fits", () => {
    const menu = useContextMenu();
    menu.menuRef.value = makeEl(160, 200);
    menu.pos.value = { x: 100, y: 100 };
    menu.clamp();
    expect(menu.pos.value).toEqual({ x: 100, y: 100 });
  });

  it("openAt clamps after the next tick", async () => {
    const menu = useContextMenu();
    menu.menuRef.value = makeEl(160, 200);
    menu.openAt({ clientX: 700, clientY: 500 } as MouseEvent);
    expect(menu.pos.value).toEqual({ x: 700, y: 500 }); // not yet clamped
    await nextTick();
    expect(menu.pos.value).toEqual({ x: 636, y: 396 }); // clamped on both axes
  });
});
