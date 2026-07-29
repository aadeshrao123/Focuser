import { i18n } from "#i18n";
import { Check, ChevronDown } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";

export interface Option {
  value: string;
  label: string;
  hint?: string;
}

/**
 * Dropdown built from buttons rather than a native `<select>`, whose popup is
 * drawn by the OS and ignores the extension's styling entirely.
 */
export function Select({
  value,
  options,
  onChange,
  ariaLabel,
  placeholder = i18n.t("select.placeholder"),
}: {
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  ariaLabel: string;
  placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const listId = useId();

  const selected = options.find((o) => o.value === value);

  useEffect(() => {
    if (!open) return;
    const onPointer = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointer);
    return () => document.removeEventListener("pointerdown", onPointer);
  }, [open]);

  useEffect(() => {
    if (open) setActive(Math.max(0, options.findIndex((o) => o.value === value)));
  }, [open, options, value]);

  function choose(index: number) {
    const option = options[index];
    if (!option) return;
    onChange(option.value);
    setOpen(false);
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (event.key === "Escape") {
      setOpen(false);
      return;
    }
    if (!open && (event.key === "Enter" || event.key === " " || event.key === "ArrowDown")) {
      event.preventDefault();
      setOpen(true);
      return;
    }
    if (!open) return;

    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((i) => (i + 1) % options.length);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((i) => (i - 1 + options.length) % options.length);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      choose(active);
    }
  }

  return (
    <div ref={root} className="relative">
      <button
        type="button"
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={onKeyDown}
        className="flex w-full items-center justify-between gap-2 rounded-xl border border-border-strong bg-elevated px-3 py-2.5 text-left text-foreground text-sm transition-colors hover:bg-hover focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary"
      >
        <span className="truncate">{selected?.label ?? placeholder}</span>
        <ChevronDown
          className={`size-4 shrink-0 text-faint-foreground transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        // biome-ignore lint/a11y/useKeyWithClickEvents: keys are handled on the trigger, which keeps focus
        <ul
          id={listId}
          // biome-ignore lint/a11y/useSemanticElements: this is the listbox
          role="listbox"
          aria-label={ariaLabel}
          className="absolute z-50 mt-1.5 max-h-56 w-full overflow-auto rounded-xl border border-border-strong bg-surface p-1 shadow-[var(--shadow-depth-lg)]"
        >
          {options.map((option, i) => {
            const isSelected = option.value === value;
            return (
              // biome-ignore lint/a11y/useKeyWithClickEvents: same
              <li
                key={option.value}
                role="option"
                aria-selected={isSelected}
                onPointerEnter={() => setActive(i)}
                onClick={() => choose(i)}
                className={`flex cursor-pointer items-center justify-between gap-2 rounded-lg px-2.5 py-2 text-sm ${
                  i === active ? "bg-hover text-foreground" : "text-muted-foreground"
                }`}
              >
                <span className="min-w-0">
                  <span className="block truncate">{option.label}</span>
                  {option.hint && (
                    <span className="block truncate text-[0.68rem] text-faint-foreground">
                      {option.hint}
                    </span>
                  )}
                </span>
                {isSelected && <Check className="size-3.5 shrink-0 text-primary" />}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
