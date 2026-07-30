export interface SelectKeyboardState {
  activeIndex: number;
  isOpen: boolean;
  optionCount: number;
  selectedIndex: number;
}

export interface SelectKeyboardResult {
  activeIndex: number;
  commitIndex: number | null;
  handled: boolean;
  isOpen: boolean;
}

function boundedIndex(index: number, optionCount: number): number {
  return Math.min(Math.max(index, 0), Math.max(optionCount - 1, 0));
}

export function resolveSelectKey(
  key: string,
  state: SelectKeyboardState,
): SelectKeyboardResult {
  const fallbackIndex = boundedIndex(state.selectedIndex, state.optionCount);
  const activeIndex = boundedIndex(state.activeIndex, state.optionCount);
  const unchanged = {
    activeIndex,
    commitIndex: null,
    handled: false,
    isOpen: state.isOpen,
  };

  if (state.optionCount === 0) {
    return unchanged;
  }

  switch (key) {
    case "ArrowDown":
      return {
        activeIndex: state.isOpen
          ? boundedIndex(activeIndex + 1, state.optionCount)
          : fallbackIndex,
        commitIndex: null,
        handled: true,
        isOpen: true,
      };
    case "ArrowUp":
      return {
        activeIndex: state.isOpen
          ? boundedIndex(activeIndex - 1, state.optionCount)
          : fallbackIndex,
        commitIndex: null,
        handled: true,
        isOpen: true,
      };
    case "Home":
      return {
        activeIndex: 0,
        commitIndex: null,
        handled: true,
        isOpen: true,
      };
    case "End":
      return {
        activeIndex: state.optionCount - 1,
        commitIndex: null,
        handled: true,
        isOpen: true,
      };
    case "Enter":
    case " ":
      return state.isOpen
        ? {
            activeIndex,
            commitIndex: activeIndex,
            handled: true,
            isOpen: false,
          }
        : {
            activeIndex: fallbackIndex,
            commitIndex: null,
            handled: true,
            isOpen: true,
          };
    case "Escape":
      return {
        activeIndex,
        commitIndex: null,
        handled: state.isOpen,
        isOpen: false,
      };
    default:
      return unchanged;
  }
}
