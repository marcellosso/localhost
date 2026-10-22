import { memo, useLayoutEffect, useMemo, useRef, useState } from "react";
import { commands, type Id, type LogLine } from "../bindings";
import { spanStyle } from "../log/ansi";
import type { LogBuffer } from "../log/buffer";
import { lineMatches, segments } from "../log/segments";
import styles from "./LogView.module.css";

/** Rows drawn above and below the visible ones, for smooth scrolling. */
const OVERSCAN = 20;
/** Space above the first line. */
const TOP_INSET = 8;

/** Which Service a line of a combined stream came from. */
export type LogPrefix = { text: string; color: string };

type Props = {
  buffer: LogBuffer;
  /** For a combined stream: each Service's Key, padded, in its colour. */
  prefixes?: ReadonlyMap<Id, LogPrefix>;
  label: string;
  /** Only lines containing this text are shown, with the matches marked. */
  filter?: string;
  /** Every line is one row of this height: no wrapping, so the list can be virtual. */
  rowHeight: number;
};

/**
 * An opaque, selectable, monospaced log. Only the rows in view are in the
 * DOM. It follows new output while scrolled to the bottom and stays put
 * while the user has scrolled up. Local URLs open in the browser.
 */
export function LogView({ buffer, prefixes, label, filter = "", rowHeight }: Props) {
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(0);
  const isFollowing = useRef(true);
  const shownDropped = useRef(buffer.dropped);

  // The lines shown: all of them, or the ones matching the filter.
  const shown = useMemo(
    () => (filter ? buffer.lines.filter((line) => lineMatches(line, filter, prefixOf(line, prefixes)?.text)) : buffer.lines),
    // `buffer.lines` changes in place; its version says when.
    [buffer, buffer.version, filter, prefixes],
  );

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
    } else if (trimmed > 0 && !filter) {
      element.scrollTop = Math.max(0, element.scrollTop - trimmed * rowHeight);
    }
    setScrollTop(element.scrollTop);
  }, [buffer, buffer.version, filter, rowHeight]);

  // A new filter shows its matches from the end.
  useLayoutEffect(() => {
    isFollowing.current = true;
  }, [filter]);

  const onScroll = () => {
    const element = scroller.current!;
    isFollowing.current = element.scrollTop + element.clientHeight >= element.scrollHeight - rowHeight;
    setScrollTop(element.scrollTop);
  };

  const count = shown.length;
  const first = Math.max(0, Math.floor((scrollTop - TOP_INSET) / rowHeight) - OVERSCAN);
  const last = Math.min(count, Math.ceil((scrollTop + height) / rowHeight) + OVERSCAN);
  const rows = [];
  for (let index = first; index < last; index++) {
    const line = shown[index];
    rows.push(
      <Row
        key={filter ? `${index}:${filter}` : buffer.dropped + index}
        line={line}
        top={TOP_INSET + index * rowHeight}
        prefix={prefixOf(line, prefixes)}
        filter={filter}
      />,
    );
  }

  return (
    <div
      ref={scroller}
      className={styles.log}
      style={{ lineHeight: `${rowHeight}px` }}
      onScroll={onScroll}
      role="log"
      // Not read out as it streams: settling Services are announced instead.
      aria-live="off"
      aria-label={label}
      tabIndex={0}
    >
      <div className={styles.content} style={{ height: TOP_INSET * 2 + count * rowHeight }}>
        {rows}
      </div>
    </div>
  );
}

function prefixOf(line: LogLine, prefixes?: ReadonlyMap<Id, LogPrefix>): LogPrefix | undefined {
  return line.serviceId ? prefixes?.get(line.serviceId) : undefined;
}

const Row = memo(function Row(props: { line: LogLine; top: number; prefix?: LogPrefix; filter: string }) {
  const { line, top, prefix, filter } = props;
  const isNote = line.channel === "localhost";
  return (
    <div className={`${styles.row} ${isNote ? styles.note : ""}`} style={{ top }}>
      <span className={styles[line.channel]} aria-hidden="true" />
      {prefix && (
        <span className={styles.prefix} style={{ color: prefix.color }}>
          {prefix.text}
        </span>
      )}
      {segments(line, filter).map((segment, index) => {
        // Links take the link colour over their ANSI one.
        const style = isNote || segment.url ? undefined : spanStyle(segment.style);
        const className = segment.isMatch ? styles.match : undefined;
        return segment.url ? (
          <a
            key={index}
            className={`${styles.link} ${className ?? ""}`}
            style={style}
            href={segment.url}
            title={`Open ${segment.url}`}
            onClick={(event) => {
              event.preventDefault();
              void commands.openUrl(segment.url!);
            }}
          >
            {segment.text}
          </a>
        ) : (
          <span key={index} className={className} style={style}>
            {segment.text}
          </span>
        );
      })}
    </div>
  );
});
