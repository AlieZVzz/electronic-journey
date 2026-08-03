import {
  type KeyboardEvent,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";

import { resolveSelectKey } from "../lib/selectControl";

export type SelectControlValue = number | string;

export interface SelectControlOption<T extends SelectControlValue = number> {
  label: string;
  value: T;
}

interface SelectControlProps<T extends SelectControlValue = number> {
  ariaLabel: string;
  disabled?: boolean;
  onChange: (value: T) => void;
  options: readonly SelectControlOption<T>[];
  value: T;
}

export function SelectControl<T extends SelectControlValue>({
  ariaLabel,
  disabled = false,
  onChange,
  options,
  value,
}: SelectControlProps<T>) {
  const listboxId = useId();
  const rootRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const selectedIndex = Math.max(
    options.findIndex((option) => option.value === value),
    0,
  );
  const [isOpen, setIsOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(selectedIndex);
  const selectedOption = options[selectedIndex];

  useEffect(() => {
    if (!isOpen) {
      return;
    }

    const closeOnOutsidePointer = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !rootRef.current?.contains(event.target)
      ) {
        setIsOpen(false);
      }
    };
    const closeOnWindowBlur = () => setIsOpen(false);

    document.addEventListener("pointerdown", closeOnOutsidePointer);
    window.addEventListener("blur", closeOnWindowBlur);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
      window.removeEventListener("blur", closeOnWindowBlur);
    };
  }, [isOpen]);

  useEffect(() => {
    if (!isOpen) {
      setActiveIndex(selectedIndex);
    }
  }, [isOpen, selectedIndex]);

  const closeAndFocusTrigger = () => {
    setIsOpen(false);
    triggerRef.current?.focus();
  };

  const commit = (index: number) => {
    const option = options[index];
    if (!option) {
      return;
    }
    onChange(option.value);
    closeAndFocusTrigger();
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key === "Tab") {
      setIsOpen(false);
      return;
    }

    const result = resolveSelectKey(event.key, {
      activeIndex,
      isOpen,
      optionCount: options.length,
      selectedIndex,
    });
    if (!result.handled) {
      return;
    }

    event.preventDefault();
    setActiveIndex(result.activeIndex);
    setIsOpen(result.isOpen);
    if (result.commitIndex !== null) {
      commit(result.commitIndex);
    }
  };

  const toggle = () => {
    if (disabled) {
      return;
    }
    setActiveIndex(selectedIndex);
    setIsOpen((open) => !open);
  };

  if (!selectedOption) {
    return null;
  }

  return (
    <span
      className={`select-control${isOpen ? " select-control--open" : ""}`}
      ref={rootRef}
    >
      <button
        aria-activedescendant={
          isOpen ? `${listboxId}-option-${activeIndex}` : undefined
        }
        aria-controls={listboxId}
        aria-expanded={isOpen}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
        className="select-control__trigger"
        disabled={disabled}
        onClick={toggle}
        onKeyDown={handleKeyDown}
        ref={triggerRef}
        role="combobox"
        type="button"
      >
        <span>{selectedOption.label}</span>
        <span aria-hidden="true" className="select-control__arrow" />
      </button>
      {isOpen && (
        <span
          aria-label={`${ariaLabel}选项`}
          className="select-control__listbox"
          id={listboxId}
          role="listbox"
        >
          {options.map((option, index) => (
            <button
              aria-selected={option.value === value}
              className={`select-control__option${
                index === activeIndex ? " is-active" : ""
              }${option.value === value ? " is-selected" : ""}`}
              id={`${listboxId}-option-${index}`}
              key={String(option.value)}
              onClick={() => commit(index)}
              onMouseEnter={() => setActiveIndex(index)}
              role="option"
              tabIndex={-1}
              type="button"
            >
              <span>{option.label}</span>
              {option.value === value && (
                <span aria-hidden="true" className="select-control__check">
                  ✓
                </span>
              )}
            </button>
          ))}
        </span>
      )}
    </span>
  );
}
