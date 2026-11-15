import { useEffect, useMemo, useRef, useState } from "react";
import type { ConsoleEntry } from "../types";
import { IconSearch, IconInfo, IconWarn, IconError } from "../icons";

interface ConsoleTabProps {
  logs: ConsoleEntry[];
  onClear: () => void;
}

const LEVELS = [
  { id: "info", label: "Info" },
  { id: "warn", label: "Warning" },
  { id: "error", label: "Error" },
  { id: "debug", label: "Debug" },
];

export default function ConsoleTab({ logs, onClear }: ConsoleTabProps) {
  const [enabled, setEnabled] = useState<Record<string, boolean>>({
    info: true,
    warn: true,
    error: true,
    debug: true,
  });
  const [query, setQuery] = useState("");
  const [autoscroll, setAutoscroll] = useState(true);
  const listRef = useRef<HTMLDivElement | null>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return logs.filter(
      (l) =>
        enabled[l.level] !== false &&
        (q === "" ||
          l.message.toLowerCase().includes(q) ||
          l.source.toLowerCase().includes(q)),
    );
  }, [logs, enabled, query]);

  useEffect(() => {
    if (autoscroll && listRef.current) {
      listRef.current.scrollTop = listRef.current.scrollHeight;
    }
  }, [filtered.length, autoscroll]);

  const copyAll = () => {
    const text = filtered.map((l) => `[${l.ts}] [${l.level.toUpperCase()}] [${l.source}] ${l.message}`).join("\n");
    navigator.clipboard?.writeText(text).catch(() => {});
  };

  const exportLogs = () => {
    const text = filtered
      .map((l) => `[${l.ts}] [${l.level.toUpperCase()}] [${l.source}] ${l.message}`)
      .join("\n");
    const blob = new Blob([text], { type: "text/plain;charset=utf-8" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `astral-launcher-log-${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-")}.txt`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  const counts = useMemo(() => {
    const c: Record<string, number> = { info: 0, warn: 0, error: 0, debug: 0 };
    for (const l of logs) {
      if (c[l.level] !== undefined) c[l.level] += 1;
    }
    return c;
  }, [logs]);

  return (
    <div className="console-wrap">
      <div className="page-title">Консоль</div>
      <div className="page-subtitle">Системные события, логи игры, предупреждения и ошибки</div>
      <div className="console-toolbar">
        <div className="row" style={{ gap: 6 }}>
          {LEVELS.map((lv) => (
            <button
              key={lv.id}
              className={`chip ${enabled[lv.id] ? "active" : ""}`}
              onClick={() => setEnabled((p) => ({ ...p, [lv.id]: !p[lv.id] }))}
            >
              {lv.label} ({counts[lv.id] ?? 0})
            </button>
          ))}
        </div>
        <div className="search-input-wrap" style={{ maxWidth: 260 }}>
          <IconSearch size={15} />
          <input
            className="input"
            placeholder="Фильтр по тексту"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <label className="checkbox-row">
          <input
            type="checkbox"
            checked={autoscroll}
            onChange={(e) => setAutoscroll(e.target.checked)}
          />
          Автопрокрутка
        </label>
        <span style={{ flex: 1 }} />
        <button className="btn sm" onClick={copyAll} title="Копировать всё">
          Копировать
        </button>
        <button className="btn sm" onClick={exportLogs} title="Сохранить лог в файл">
          Экспорт
        </button>
        <button className="btn sm danger" onClick={onClear}>
          Очистить
        </button>
      </div>
      <div className="console-list" ref={listRef}>
        {filtered.length === 0 ? (
          <div style={{ padding: 24, color: "var(--text-faint)", textAlign: "center", fontFamily: "var(--font)" }}>
            Логов нет. Установите версию или запустите игру.
          </div>
        ) : (
          filtered.map((l, i) => (
            <div className={`console-line ${l.level}`} key={i}>
              <span className="ts">{l.ts}</span>
              <span className={`lvl-badge ${l.level}`}>
                {l.level === "info" ? (
                  <IconInfo size={10} />
                ) : l.level === "warn" ? (
                  <IconWarn size={10} />
                ) : l.level === "error" ? (
                  <IconError size={10} />
                ) : (
                  "DBG"
                )}
              </span>
              <span className="src">{l.source}</span>
              <span className="msg">{l.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
