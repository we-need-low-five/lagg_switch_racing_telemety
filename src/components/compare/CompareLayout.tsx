import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

interface ResizeSplitterProps {
  orientation: "horizontal" | "vertical";
  onDrag: (deltaPx: number) => void;
  onDragEnd?: () => void;
}

export function ResizeSplitter({
  orientation,
  onDrag,
  onDragEnd,
}: ResizeSplitterProps) {
  const dragging = useRef(false);
  const lastPos = useRef(0);

  const onMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      dragging.current = true;
      lastPos.current = orientation === "vertical" ? e.clientX : e.clientY;

      function onMouseMove(ev: MouseEvent) {
        if (!dragging.current) return;
        const pos = orientation === "vertical" ? ev.clientX : ev.clientY;
        const delta = pos - lastPos.current;
        lastPos.current = pos;
        onDrag(delta);
      }

      function onMouseUp() {
        dragging.current = false;
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        onDragEnd?.();
      }

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [orientation, onDrag, onDragEnd],
  );

  return (
    <div
      className={`resize-splitter resize-splitter-${orientation}`}
      onMouseDown={onMouseDown}
      role="separator"
      aria-orientation={orientation}
    />
  );
}

interface CollapsiblePanelProps {
  title: string;
  collapsed: boolean;
  onToggle: () => void;
  toolbar?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function CollapsiblePanel({
  title,
  collapsed,
  onToggle,
  toolbar,
  children,
  className = "",
}: CollapsiblePanelProps) {
  return (
    <div
      className={`collapsible-panel ${collapsed ? "collapsed" : ""} ${className}`}
    >
      <div className="collapsible-panel-header">
        <button
          type="button"
          className="collapsible-panel-toggle"
          onClick={onToggle}
          aria-expanded={!collapsed}
        >
          <span className="collapsible-chevron">{collapsed ? "▸" : "▾"}</span>
          <span>{title}</span>
        </button>
        {toolbar && !collapsed && (
          <div className="collapsible-panel-toolbar">{toolbar}</div>
        )}
      </div>
      {!collapsed && (
        <div className="collapsible-panel-body">{children}</div>
      )}
    </div>
  );
}

function findChartsScrollParent(el: HTMLElement | null): HTMLElement | null {
  while (el) {
    if (el.classList.contains("compare-charts-scroll")) return el;
    el = el.parentElement;
  }
  return null;
}

interface CollapsibleChartProps {
  title: string;
  collapsed: boolean;
  height: number;
  onToggle: () => void;
  onResizeCommit: (newHeight: number) => void;
  children: (displayHeight: number) => ReactNode;
}

export function CollapsibleChart({
  title,
  collapsed,
  height,
  onToggle,
  onResizeCommit,
  children,
}: CollapsibleChartProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [displayHeight, setDisplayHeight] = useState(height);
  const scrollParentRef = useRef<HTMLElement | null>(null);
  const scrollTopRef = useRef(0);

  useEffect(() => {
    setDisplayHeight(height);
  }, [height]);

  const preserveScroll = useCallback(() => {
    const scrollParent = scrollParentRef.current;
    if (scrollParent) {
      scrollParent.scrollTop = scrollTopRef.current;
    }
  }, []);

  const onResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      scrollParentRef.current = findChartsScrollParent(rootRef.current);
      scrollTopRef.current = scrollParentRef.current?.scrollTop ?? 0;

      const startY = e.clientY;
      const startHeight = displayHeight;
      let lastHeight = startHeight;

      function onMouseMove(ev: MouseEvent) {
        const delta = ev.clientY - startY;
        lastHeight = Math.max(120, startHeight + delta);
        setDisplayHeight(lastHeight);
        requestAnimationFrame(preserveScroll);
      }

      function onMouseUp() {
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
        onResizeCommit(lastHeight);
        requestAnimationFrame(preserveScroll);
      }

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [displayHeight, onResizeCommit, preserveScroll],
  );

  return (
    <div
      ref={rootRef}
      className={`collapsible-chart ${collapsed ? "collapsed" : ""}`}
    >
      <div className="collapsible-chart-header">
        <button
          type="button"
          className="collapsible-chart-toggle"
          onClick={onToggle}
          aria-expanded={!collapsed}
        >
          <span className="collapsible-chevron">{collapsed ? "▸" : "▾"}</span>
          <span>{title}</span>
        </button>
      </div>
      {!collapsed && (
        <>
          <div className="collapsible-chart-body">{children(displayHeight)}</div>
          <div
            className="chart-resize-handle"
            onMouseDown={onResizeMouseDown}
            role="separator"
            aria-orientation="horizontal"
          />
        </>
      )}
    </div>
  );
}
