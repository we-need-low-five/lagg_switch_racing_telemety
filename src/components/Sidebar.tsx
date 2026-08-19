import { useRef, useState } from "react";
import { NavLink } from "react-router-dom";

const HOVER_DELAY_MS = 120;

const MAIN_NAV_ITEMS = [
  { to: "/", end: true, label: "Sessions", icon: "🏁" },
  { to: "/leaderboard", end: false, label: "Leaderboard", icon: "⏱" },
  { to: "/compare", end: true, label: "Compare", icon: "📊" },
  { to: "/fuel", end: false, label: "Fuel", icon: "⛽" },
] as const;

const BOTTOM_NAV_ITEMS = [
  { to: "/settings", end: false, label: "Settings", icon: "🛠" },
] as const;

export function Sidebar() {
  const [hoverPreview, setHoverPreview] = useState(false);
  const hoverTimer = useRef<number | null>(null);

  function onMouseEnter() {
    hoverTimer.current = window.setTimeout(() => setHoverPreview(true), HOVER_DELAY_MS);
  }

  function onMouseLeave() {
    if (hoverTimer.current) {
      window.clearTimeout(hoverTimer.current);
      hoverTimer.current = null;
    }
    setHoverPreview(false);
  }

  return (
    <nav
      className={`sidebar icon-rail ${hoverPreview ? "show-labels" : ""}`}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      <div className="sidebar-top">
        <div className="brand">
          <span className="brand-mark">
            <img src="/lagg-logo.png" alt="LAGG" className="brand-logo" />
          </span>
          <div className="brand-text">
            <strong>SimTelemetry</strong>
            <small>Live telemetry recorder</small>
          </div>
        </div>

        {MAIN_NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            title={item.label}
            className="nav-item"
          >
            <span className="nav-icon" aria-hidden>{item.icon}</span>
            <span className="nav-label">{item.label}</span>
          </NavLink>
        ))}
      </div>

      <div className="sidebar-bottom">
        {BOTTOM_NAV_ITEMS.map((item) => (
          <NavLink
            key={item.to}
            to={item.to}
            end={item.end}
            title={item.label}
            className="nav-item"
          >
            <span className="nav-icon" aria-hidden>{item.icon}</span>
            <span className="nav-label">{item.label}</span>
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
