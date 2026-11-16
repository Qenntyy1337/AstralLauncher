import { useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import { IconCancel, IconChevron } from "../icons";

export function ProgressBar({ value, className }: { value: number; className?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div className={`progress-track ${className ?? ""}`}>
      <div className="progress-fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

export function Spinner() {
  return <span className="spinner" />;
}

export function EmptyState({
  icon,
  title,
  hint,
}: {
  icon: React.ReactNode;
  title: string;
  hint?: string;
}) {
  return (
    <div className="empty-state">
      <div style={{ opacity: 0.5 }}>{icon}</div>
      <div style={{ fontSize: 14, fontWeight: 600 }}>{title}</div>
      {hint ? <div style={{ fontSize: 12.5 }}>{hint}</div> : null}
    </div>
  );
}

export function IconButton({
  icon,
  title,
  onClick,
  disabled,
  danger,
}: {
  icon: React.ReactNode;
  title: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      className={`btn sm ${danger ? "danger" : ""}`}
      style={{ padding: 8, width: 32, height: 32 }}
      title={title}
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
    </button>
  );
}

export function CancelButton({ onClick, title }: { onClick: () => void; title: string }) {
  return (
    <button className="btn sm danger" title={title} onClick={onClick}>
      <IconCancel size={14} />
    </button>
  );
}

export function AccountHead({ name, size = 32 }: { name: string; size?: number }) {
  const [failed, setFailed] = useState(false);
  if (failed || !name) {
    return (
      <div
        style={{
          width: size,
          height: size,
          borderRadius: size / 4,
          background: "var(--bg-active)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: size * 0.45,
          fontWeight: 700,
          color: "var(--accent)",
          flexShrink: 0,
        }}
      >
        {(name || "?").slice(0, 1).toUpperCase()}
      </div>
    );
  }
  return (
    <img
      src={`https://mc-heads.net/avatar/${encodeURIComponent(name)}/${size * 2 > 64 ? 64 : Math.max(32, size)}`}
      alt={name}
      width={size}
      height={size}
      style={{
        width: size,
        height: size,
        borderRadius: size / 4,
        flexShrink: 0,
        imageRendering: "pixelated" as const,
        background: "var(--bg-active)",
      }}
      onError={() => setFailed(true)}
    />
  );
}

export interface DropdownOption {
  value: string;
  label: string;
}

export function Dropdown({
  value,
  options,
  onChange,
  style,
  title,
  minWidth,
}: {
  value: string;
  options: DropdownOption[];
  onChange: (v: string) => void;
  style?: CSSProperties;
  title?: string;
  minWidth?: number;
}) {
  const [open, setOpen] = useState(false);
  const [rect, setRect] = useState<{ left: number; top: number; width: number } | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!ref.current?.contains(target) && !menuRef.current?.contains(target)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const onScroll = () => setOpen(false);
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey);
    window.addEventListener("resize", onScroll);
    window.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("scroll", onScroll, true);
    };
  }, [open]);

  const toggle = () => {
    const el = ref.current;
    if (el) {
      const r = el.getBoundingClientRect();
      const menuH = Math.min(options.length * 32 + 14, 300);
      const fitsBelow = r.bottom + menuH + 8 <= window.innerHeight;
      setRect({
        left: Math.max(8, Math.min(r.left, window.innerWidth - r.width - 8)),
        top: fitsBelow ? r.bottom + 6 : Math.max(8, r.top - menuH - 6),
        width: r.width,
      });
    }
    setOpen((o) => !o);
  };

  const current = options.find((o) => o.value === value);
  return (
    <div ref={ref} className="dd" style={style}>
      <button
        type="button"
        className="dd-btn"
        title={title}
        style={minWidth ? { minWidth } : undefined}
        onClick={toggle}
      >
        <span className="dd-label">{current ? current.label : ""}</span>
        <IconChevron size={14} className={open ? "dd-chev open" : "dd-chev"} />
      </button>
      {open && rect
        ? createPortal(
            <div
              ref={menuRef}
              className="dd-menu"
              style={{ left: rect.left, top: rect.top, width: rect.width }}
            >
              {options.map((o) => (
                <button
                  key={o.value}
                  type="button"
                  className={o.value === value ? "dd-item active" : "dd-item"}
                  onClick={() => {
                    setOpen(false);
                    onChange(o.value);
                  }}
                >
                  {o.label}
                </button>
              ))}
            </div>,
            document.body,
          )
        : null}
    </div>
  );
}
