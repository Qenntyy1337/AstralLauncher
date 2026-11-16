import { useState } from "react";
import { IconCancel } from "../icons";

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
