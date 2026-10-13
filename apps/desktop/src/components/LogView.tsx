import { memo, useLayoutEffect, useRef, useState } from "react";
import type { Id, LogLine } from "../bindings";
import { spanStyle } from "../log/ansi";
import type { LogBuffer } from "../log/buffer";
import styles from "./LogView.module.css";

/** Every line is one row of this height: no wrapping, so the list can be virtual. */
export const ROW_HEIGHT = 18;
/** Rows drawn above and below the visible ones, for smooth scrolling. */
const OVERSCAN = 20;

/** Which Service a line of a combined stream came from. */
export type LogPrefix = { text: string; color: string };

type Props = {
  buffer: LogBuffer;
  /** For a combined stream: each Service's Key, padded, in its colour. */
  prefixes?: ReadonlyMap<Id, LogPrefix>;
  label: string;
  emptyText: string;
};

/**
 * An opaque, selectable, monospaced log. Only the rows in view are in the
 * DOM. It follows new output while scrolled to the bottom and stays put
 * while the user has scrolled up.
 */
export function LogView({ buffer, prefixes, label, emptyText }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);
  const isFollowing = useRef(true);
  const shownDropped = useRef(buffer.dropped);

  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setHeight(element.clientHeight));
    observer.observe(element);
    setHeight(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  // After new lines: follow the end, or keep the same lines in view when
  // older ones were trimmed above them.
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    const trimmed = buffer.dropped - shownDropped.current;
    shownDropped.current = buffer.dropped;
    if (isFollowing.current) {
      element.scrollTop = element.scrollHeight;
    } else if (trimmed > 0) {
      element.scrollTop = Math.max(0, element.scrollTop - trimmed * ROW_HEIGHT);
    }
    setScrollTop(element.scrollTop);
  }, [buffer, buffer.version]);

  const onScroll = () => {
    const element = scroller.current!;
    isFollowing.current = element.scrollTop + element.clientHeight >= element.scrollHeight - ROW_HEIGHT;
    setScrollTop(element.scrollTop);
  };

  const count = buffer.lines.length;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(count, Math.ceil((scrollTop + height) / ROW_HEIGHT) + OVERSCAN);
  const rows = [];
  for (let index = first; index < last; index++) {
    const line = buffer.lines[index];
    rows.push(
      <Row
        key={buffer.dropped + index}
        line={line}
        top={index * ROW_HEIGHT}
        prefix={line.serviceId ? prefixes?.get(line.serviceId) : undefined}
      />,
    );
  }

  return (
    <div ref={scroller} className={styles.log} onScroll={onScroll} role="log" aria-label={label} tabIndex={0}>
      {count === 0 ? (
        <p className={styles.empty}>{emptyText}</p>
      ) : (
        <div className={styles.content} style={{ height: count * ROW_HEIGHT }}>
          {rows}
        </div>
      )}
    </div>
  );
}

const Row = memo(function Row({ line, top, prefix }: { line: LogLine; top: number; prefix?: LogPrefix }) {
  return (
    <div className={styles.row} style={{ top }}>
      <span className={styles[line.channel]} aria-hidden="true" />
      {prefix && (
        <span className={styles.prefix} style={{ color: prefix.color }}>
          {prefix.text}
        </span>
      )}
      {line.spans.map((span, index) => (
        <span key={index} style={spanStyle(span.style)}>
          {span.text}
        </span>
      ))}
    </div>
  );
});
