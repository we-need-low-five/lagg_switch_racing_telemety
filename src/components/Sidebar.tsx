import { useRef, useState } from "react";
import { NavLink } from "react-router-dom";
import { NavIcon, type NavIconId } from "./NavIcons";

const HOVER_DELAY_MS = 120;

const MAIN_NAV_ITEMS: Array<{
  to: string;
  end: boolean;
  label: string;
  icon: NavIconId;
}> = [
  { to: "/", end: true, label: "Sessions", icon: "sessions" },
  { to: "/leaderboard", end: false, label: "Leaderboard", icon: "leaderboard" },
  { to: "/compare", end: true, label: "Compare", icon: "compare" },
  { to: "/fuel", end: false, label: "Fuel", icon: "fuel" },
];

const BOTTOM_NAV_ITEMS: Array<{
  to: string;
  end: boolean;
  label: string;
  icon: NavIconId;
}> = [{ to: "/settings", end: false, label: "Settings", icon: "settings" }];

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
            <span className="nav-icon" aria-hidden>
              <NavIcon id={item.icon} />
            </span>
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
            <span className="nav-icon" aria-hidden>
              <NavIcon id={item.icon} />
            </span>
            <span className="nav-label">{item.label}</span>
          </NavLink>
        ))}
      </div>
    </nav>
  );
}
