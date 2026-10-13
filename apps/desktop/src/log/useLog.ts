import { Channel } from "@tauri-apps/api/core";
import { useEffect, useReducer, useState } from "react";
import { commands, type LogBatch, type LogSource } from "../bindings";
import { LogBuffer } from "./buffer";

let received = 0;

/** Lines received by every log so far, for the performance measurement. */
export function linesReceived(): number {
  return received;
}

/**
 * The lines of `source`, live. The core sends batches at most once per frame;
 * React re-renders at most once per animation frame on top of that. `key`
 * names the source: a new key opens a new stream.
 */
export function useLog(key: string, source: LogSource | null): LogBuffer {
  const [buffer, setBuffer] = useState(() => new LogBuffer());
  // A counter, not the buffer's version: each stream's buffer counts from 0,
  // and setting the same number twice wouldn't re-render.
  const [, rerender] = useReducer((count: number) => count + 1, 0);

  useEffect(() => {
    const next = new LogBuffer();
    setBuffer(next);
    if (!source) return;

    let frame: number | null = null;
    let id: number | null = null;
    let isClosed = false;
    const batches = new Channel<LogBatch>();
    batches.onmessage = (batch) => {
      received += batch.lines.length + batch.skipped;
      next.append(batch);
      frame ??= requestAnimationFrame(() => {
        frame = null;
        rerender();
      });
    };
    void commands.openLog(source, batches).then((result) => {
      if (result.status !== "ok") return;
      if (isClosed) void commands.closeLog(result.data);
      else id = result.data;
    });
    return () => {
      isClosed = true;
      if (frame !== null) cancelAnimationFrame(frame);
      if (id !== null) void commands.closeLog(id);
    };
    // `key` stands for `source`, which is a new object on every render.
  }, [key]);

  return buffer;
}
