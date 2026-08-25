import { type PointerEvent, useEffect, useId, useRef, useState } from "react";
import {
  hexToHsl,
  hslToHex,
  parseHex,
  type Hsl,
} from "../lib/theme";

interface ColorPickerProps {
  value: string;
  onChange: (hex: string) => void;
  "aria-label"?: string;
}

type Hsv = { h: number; s: number; v: number };

function clamp(n: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, n));
}

function wrapHue(h: number): number {
  return ((h % 360) + 360) % 360;
}

function hslToHsv(hsl: Hsl): Hsv {
  const l = clamp(hsl.l, 0, 100) / 100;
  const s = clamp(hsl.s, 0, 100) / 100;
  const v = l + s * Math.min(l, 1 - l);
  const sv = v === 0 ? 0 : 2 * (1 - l / v);
  return { h: wrapHue(hsl.h), s: sv * 100, v: v * 100 };
}

function hsvToHsl(hsv: Hsv): Hsl {
  const s = clamp(hsv.s, 0, 100) / 100;
  const v = clamp(hsv.v, 0, 100) / 100;
  const l = v * (1 - s / 2);
  const sl = l === 0 || l === 1 ? 0 : (v - l) / Math.min(l, 1 - l);
  return { h: wrapHue(hsv.h), s: sl * 100, l: l * 100 };
}

function colorFromHex(hex: string): { hsl: Hsl; hsv: Hsv; hex: string } {
  const normalized = parseHex(hex) ?? "#000000";
  const hsl = hexToHsl(normalized) ?? { h: 0, s: 0, l: 0 };
  return { hsl, hsv: hslToHsv(hsl), hex: normalized };
}

export function ColorPicker({
  value,
  onChange,
  "aria-label": ariaLabel,
}: ColorPickerProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const planeRef = useRef<HTMLDivElement>(null);
  const baseId = useId();
  const [open, setOpen] = useState(false);
  const parsed = colorFromHex(value);
  const [hsl, setHsl] = useState<Hsl>(parsed.hsl);
  const [hsv, setHsv] = useState<Hsv>(parsed.hsv);
  const [hexDraft, setHexDraft] = useState(parsed.hex);

  useEffect(() => {
    const next = colorFromHex(value);
    setHsl(next.hsl);
    setHsv(next.hsv);
    setHexDraft(next.hex);
  }, [value]);

  useEffect(() => {
    if (!open) return;
    function onDocMouseDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    document.addEventListener("mousedown", onDocMouseDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onDocMouseDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  function commitHsl(next: Hsl) {
    const hslNext = {
      h: wrapHue(next.h),
      s: clamp(next.s, 0, 100),
      l: clamp(next.l, 0, 100),
    };
    const hex = hslToHex(hslNext);
    setHsl(hslNext);
    setHsv(hslToHsv(hslNext));
    setHexDraft(hex);
    onChange(hex);
  }

  function commitHsv(next: Hsv) {
    commitHsl(hsvToHsl(next));
  }

  function applyPointer(clientX: number, clientY: number) {
    const el = planeRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const x = clamp((clientX - rect.left) / rect.width, 0, 1);
    const y = clamp((clientY - rect.top) / rect.height, 0, 1);
    commitHsv({ h: hsv.h, s: x * 100, v: (1 - y) * 100 });
  }

  function onPlanePointerDown(e: PointerEvent<HTMLDivElement>) {
    e.currentTarget.setPointerCapture(e.pointerId);
    applyPointer(e.clientX, e.clientY);
  }

  function onPlanePointerMove(e: PointerEvent<HTMLDivElement>) {
    if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
    applyPointer(e.clientX, e.clientY);
  }

  function onHexChange(raw: string) {
    setHexDraft(raw);
    const next = parseHex(raw);
    if (!next) return;
    const parsedNext = colorFromHex(next);
    setHsl(parsedNext.hsl);
    setHsv(parsedNext.hsv);
    onChange(next);
  }

  return (
    <div className="color-picker" ref={rootRef}>
      <button
        type="button"
        className="color-picker-swatch"
        style={{ background: parsed.hex }}
        aria-label={ariaLabel ?? "Open color picker"}
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((v) => !v)}
      />
      {open && (
        <div
          className="color-picker-popover"
          role="dialog"
          aria-label={ariaLabel ?? "Color picker"}
        >
          <div
            ref={planeRef}
            className="color-picker-plane"
            style={{
              background: `linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, hsl(${hsv.h} 100% 50%))`,
            }}
            onPointerDown={onPlanePointerDown}
            onPointerMove={onPlanePointerMove}
          >
            <span
              className="color-picker-plane-thumb"
              style={{ left: `${hsv.s}%`, top: `${100 - hsv.v}%` }}
            />
          </div>
          <input
            type="range"
            className="color-picker-hue"
            min={0}
            max={360}
            step={1}
            value={Math.round(hsv.h)}
            aria-label="Hue"
            onChange={(e) =>
              commitHsv({ ...hsv, h: Number(e.target.value) })
            }
          />
          <div className="color-picker-fields">
            <label className="color-picker-field" htmlFor={`${baseId}-h`}>
              H
              <input
                id={`${baseId}-h`}
                type="number"
                min={0}
                max={360}
                step={1}
                value={Math.round(hsl.h)}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  if (!Number.isFinite(n)) return;
                  commitHsl({ ...hsl, h: n });
                }}
              />
            </label>
            <label className="color-picker-field" htmlFor={`${baseId}-s`}>
              S
              <input
                id={`${baseId}-s`}
                type="number"
                min={0}
                max={100}
                step={1}
                value={Math.round(hsl.s)}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  if (!Number.isFinite(n)) return;
                  commitHsl({ ...hsl, s: n });
                }}
              />
            </label>
            <label className="color-picker-field" htmlFor={`${baseId}-l`}>
              L
              <input
                id={`${baseId}-l`}
                type="number"
                min={0}
                max={100}
                step={1}
                value={Math.round(hsl.l)}
                onChange={(e) => {
                  const n = Number(e.target.value);
                  if (!Number.isFinite(n)) return;
                  commitHsl({ ...hsl, l: n });
                }}
              />
            </label>
            <label className="color-picker-field color-picker-field-hex" htmlFor={`${baseId}-hex`}>
              Hex
              <input
                id={`${baseId}-hex`}
                type="text"
                spellCheck={false}
                value={hexDraft}
                onChange={(e) => onHexChange(e.target.value)}
                onBlur={() => setHexDraft(parsed.hex)}
              />
            </label>
          </div>
        </div>
      )}
    </div>
  );
}
