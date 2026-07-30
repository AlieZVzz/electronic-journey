import { describe, expect, it } from "vitest";

import { resolveSelectKey } from "./selectControl";

const closedState = {
  activeIndex: 2,
  isOpen: false,
  optionCount: 5,
  selectedIndex: 2,
};

describe("select control keyboard behavior", () => {
  it("opens on arrows and moves through options while open", () => {
    expect(resolveSelectKey("ArrowDown", closedState)).toMatchObject({
      activeIndex: 2,
      handled: true,
      isOpen: true,
    });
    expect(
      resolveSelectKey("ArrowDown", { ...closedState, isOpen: true }),
    ).toMatchObject({
      activeIndex: 3,
      handled: true,
      isOpen: true,
    });
    expect(
      resolveSelectKey("ArrowUp", {
        ...closedState,
        activeIndex: 0,
        isOpen: true,
      }),
    ).toMatchObject({
      activeIndex: 0,
      isOpen: true,
    });
  });

  it("supports Home and End navigation", () => {
    expect(resolveSelectKey("Home", closedState).activeIndex).toBe(0);
    expect(resolveSelectKey("End", closedState).activeIndex).toBe(4);
  });

  it("commits with Enter or Space and cancels with Escape", () => {
    const openState = { ...closedState, activeIndex: 3, isOpen: true };

    expect(resolveSelectKey("Enter", openState)).toMatchObject({
      commitIndex: 3,
      handled: true,
      isOpen: false,
    });
    expect(resolveSelectKey(" ", openState).commitIndex).toBe(3);
    expect(resolveSelectKey("Escape", openState)).toMatchObject({
      commitIndex: null,
      handled: true,
      isOpen: false,
    });
  });

  it("leaves unrelated keys to the browser", () => {
    expect(resolveSelectKey("Tab", closedState)).toMatchObject({
      commitIndex: null,
      handled: false,
      isOpen: false,
    });
  });
});
