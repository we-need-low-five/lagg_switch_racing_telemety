import { useEffect, useRef, useState } from "react";

interface OverflowMenuItem {
  label: string;
  onClick: () => void;
  danger?: boolean;
}

interface OverflowMenuProps {
  items: OverflowMenuItem[];
}

export function OverflowMenu({ items }: OverflowMenuProps) {
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDocClick(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", onDocClick);
    return () => document.removeEventListener("mousedown", onDocClick);
  }, [open]);

  return (
    <div className="overflow-menu" ref={rootRef}>
      <button
        type="button"
        className="secondary overflow-menu-trigger"
        onClick={() => setOpen((v) => !v)}
        aria-label="More actions"
        aria-expanded={open}
      >
        ⋮
      </button>
      {open && (
        <div className="overflow-menu-dropdown">
          {items.map((item) => (
            <button
              key={item.label}
              type="button"
              className={item.danger ? "danger" : "secondary"}
              onClick={() => {
                setOpen(false);
                item.onClick();
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
