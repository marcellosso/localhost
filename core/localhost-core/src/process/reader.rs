//! Reads a Run's pipes and splits them into lines, delivered a read at a time.

use std::os::fd::OwnedFd;
use std::sync::Arc;

use tokio::io::AsyncReadExt;
use tokio::net::unix::pipe;
use tokio::sync::{Semaphore, mpsc, watch};

use super::MAX_LINE_LENGTH;
use crate::output::{OutputChannel, OutputLine};

/// How many batches read but not yet taken a process tree may have before
/// its readers pause: about 1 MB of output at 64 KB per read. A command that
/// prints faster than localhost keeps up then blocks on its pipe, as it would
/// in a terminal, instead of growing localhost's memory.
pub(crate) const PENDING_BATCHES: usize = 16;

/// Splits bytes into lines as they arrive.
#[derive(Default)]
pub(crate) struct LineSplitter {
    buffer: Vec<u8>,
}

impl LineSplitter {
    /// The complete lines in `chunk` and what came before it. A newline within
    /// the limit ends a line; past the limit, a piece is cut.
    pub(crate) fn push(&mut self, chunk: &[u8], channel: OutputChannel) -> Vec<OutputLine> {
        self.buffer.extend_from_slice(chunk);
        let mut lines = Vec::new();
        let mut start = 0;
        loop {
            let window_end = self.buffer.len().min(start + MAX_LINE_LENGTH + 1);
            if let Some(offset) = self.buffer[start..window_end].iter().position(|&b| b == b'\n') {
                lines.push(make_line(&self.buffer[start..start + offset], channel));
                start += offset + 1;
            } else if self.buffer.len() - start > MAX_LINE_LENGTH {
                let end = start + piece_length(&self.buffer[start..]);
                lines.push(make_line(&self.buffer[start..end], channel));
                start = end;
            } else {
                break;
            }
        }
        self.buffer.drain(..start);
        lines
    }

    /// The partial line left over, if any.
    pub(crate) fn finish(&mut self, channel: OutputChannel) -> Option<OutputLine> {
        if self.buffer.is_empty() {
            return None;
        }
        let line = make_line(&self.buffer, channel);
        self.buffer.clear();
        Some(line)
    }
}

/// How much of a line longer than `MAX_LINE_LENGTH` to deliver as a piece: at
/// most that many bytes, backing off so a UTF-8 character isn't split between
/// pieces (which would show as two U+FFFD). Invalid UTF-8 is cut at the limit.
pub(crate) fn piece_length(bytes: &[u8]) -> usize {
    let is_continuation = |offset: usize| bytes[offset] & 0b1100_0000 == 0b1000_0000;
    let mut length = MAX_LINE_LENGTH;
    // A continuation byte (10xxxxxx) can't start a character; a character is at most 4 bytes.
    while length > MAX_LINE_LENGTH - 3 && is_continuation(length) {
        length -= 1;
    }
    if is_continuation(length) { MAX_LINE_LENGTH } else { length }
}

fn make_line(bytes: &[u8], channel: OutputChannel) -> OutputLine {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    OutputLine::new(channel, String::from_utf8_lossy(bytes))
}

/// Reads `fd` until it ends or `cancel` turns true, sending each read's lines
/// as one batch. Before each read it takes one of `flow`'s permits, which the
/// consumer gives back for every batch it takes. On cancel, the partial line
/// is delivered.
pub(crate) async fn read_lines(
    fd: OwnedFd,
    channel: OutputChannel,
    batches: mpsc::UnboundedSender<Vec<OutputLine>>,
    flow: Arc<Semaphore>,
    mut cancel: watch::Receiver<bool>,
) {
    let mut splitter = LineSplitter::default();
    let mut chunk = vec![0u8; 64 * 1024];
    let Ok(mut pipe) = pipe::Receiver::from_owned_fd(fd) else { return };
    loop {
        let permit = tokio::select! {
            biased;
            _ = cancel.wait_for(|cancelled| *cancelled) => break,
            permit = flow.acquire() => permit,
        };
        // Taken back by the consumer, one per batch it takes.
        if let Ok(permit) = permit {
            permit.forget();
        }
        let read = tokio::select! {
            biased;
            _ = cancel.wait_for(|cancelled| *cancelled) => break,
            read = pipe.read(&mut chunk) => read,
        };
        match read {
            Ok(0) | Err(_) => break,
            Ok(count) => {
                let lines = splitter.push(&chunk[..count], channel);
                if lines.is_empty() {
                    flow.add_permits(1);
                } else if batches.send(lines).is_err() {
                    // Nobody takes the output any more: keep draining so the command never blocks.
                    flow.add_permits(1);
                }
            }
        }
    }
    if let Some(line) = splitter.finish(channel) {
        let _ = batches.send(vec![line]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: Vec<OutputLine>) -> Vec<String> {
        lines.into_iter().map(|line| line.text).collect()
    }

    #[test]
    fn splits_lines_keeping_empty_ones_and_stripping_carriage_returns() {
        let mut splitter = LineSplitter::default();
        assert_eq!(texts(splitter.push(b"a\r\n\nb\nno new", OutputChannel::Stdout)), ["a", "", "b"]);
        assert_eq!(texts(splitter.push(b"line", OutputChannel::Stdout)), Vec::<String>::new());
        assert_eq!(splitter.finish(OutputChannel::Stdout).map(|line| line.text), Some("no newline".into()));
        assert_eq!(splitter.finish(OutputChannel::Stdout), None);
    }

    #[test]
    fn replaces_invalid_utf8() {
        let mut splitter = LineSplitter::default();
        assert_eq!(texts(splitter.push(b"caf\xe9\n", OutputChannel::Stdout)), ["caf\u{FFFD}"]);
    }

    #[test]
    fn a_line_is_cut_at_the_limit_even_when_its_end_arrives_later() {
        let mut splitter = LineSplitter::default();
        let first = splitter.push(&vec![b'x'; MAX_LINE_LENGTH], OutputChannel::Stdout);
        assert!(first.is_empty(), "exactly the limit waits for more");
        let rest = splitter.push(b"yyyy\n", OutputChannel::Stdout);
        assert_eq!(texts(rest), ["x".repeat(MAX_LINE_LENGTH), "yyyy".into()]);
    }

    #[test]
    fn long_lines_are_cut_at_a_character_boundary() {
        let plain = vec![b'x'; MAX_LINE_LENGTH + 1];
        assert_eq!(piece_length(&plain), MAX_LINE_LENGTH);
        // "€" is E2 82 AC: the limit falls after its first byte, so the piece ends before it.
        let mut euro = vec![b'x'; MAX_LINE_LENGTH - 1];
        euro.extend_from_slice("€".as_bytes());
        assert_eq!(piece_length(&euro), MAX_LINE_LENGTH - 1);
        // Only continuation bytes: not UTF-8, cut at the limit.
        assert_eq!(piece_length(&vec![0x80; MAX_LINE_LENGTH + 1]), MAX_LINE_LENGTH);
    }

    #[test]
    fn a_long_line_of_multibyte_characters_splits_without_replacement_characters() {
        let line = "€".repeat(50_000);
        let mut splitter = LineSplitter::default();
        let mut pieces = splitter.push(format!("{line}\n").as_bytes(), OutputChannel::Stdout);
        pieces.extend(splitter.finish(OutputChannel::Stdout));
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|piece| piece.text.len() <= MAX_LINE_LENGTH));
        assert_eq!(texts(pieces).concat(), line);
    }
}
