//! Lightweight ANSI handling for log lines: SGR colours and attributes become
//! styles; every other escape sequence and control character is removed.

use serde::{Deserialize, Serialize};

/// A colour from an ANSI SGR sequence. Clients map indexed colours to their theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Enum))]
pub enum AnsiColor {
    /// 0–7 standard, 8–15 bright, 16–255 the xterm 256-colour palette.
    Indexed {
        index: u8,
    },
    Rgb {
        red: u8,
        green: u8,
        blue: u8,
    },
}

/// The text attributes set by ANSI SGR sequences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct TextStyle {
    pub foreground: Option<AnsiColor>,
    pub background: Option<AnsiColor>,
    pub is_bold: bool,
    pub is_dim: bool,
    pub is_italic: bool,
    pub is_underlined: bool,
    pub is_inverse: bool,
    pub is_strikethrough: bool,
}

/// A run of identically styled text.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Span {
    pub text: String,
    pub style: TextStyle,
}

/// One line of output split into runs of identically styled text.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct StyledLine {
    pub spans: Vec<Span>,
}

impl StyledLine {
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// Each line is parsed on its own, starting unstyled. A carriage return keeps
/// only what follows it, as a terminal would show a redrawn progress line.
pub struct Ansi;

impl Ansi {
    pub fn parse(line: &str) -> StyledLine {
        let mut parser = Parser::default();
        parser.parse(&line.chars().collect::<Vec<char>>());
        StyledLine { spans: parser.spans }
    }

    pub fn strip(line: &str) -> String {
        Ansi::parse(line).plain_text()
    }
}

#[derive(Default)]
struct Parser {
    spans: Vec<Span>,
    style: TextStyle,
}

impl Parser {
    fn parse(&mut self, chars: &[char]) {
        let mut index = 0;
        while index < chars.len() {
            match chars[index] {
                '\u{1B}' => {
                    index = self.skip_escape(chars, index + 1);
                    continue;
                }
                '\r' => self.spans.clear(),
                '\t' => self.append('\t'),
                c if c as u32 >= 0x20 && c as u32 != 0x7F => self.append(c),
                _ => {}
            }
            index += 1;
        }
    }

    /// Consumes the escape sequence after an ESC and returns the index after it.
    fn skip_escape(&mut self, chars: &[char], start: usize) -> usize {
        let Some(&first) = chars.get(start) else { return start };
        match first {
            '[' => {
                // CSI: parameter and intermediate bytes, then a final byte in 0x40–0x7E.
                let mut index = start + 1;
                while index < chars.len() && !(0x40..=0x7E).contains(&(chars[index] as u32)) {
                    index += 1;
                }
                if index >= chars.len() {
                    return index;
                }
                if chars[index] == 'm' {
                    let parameters: String = chars[start + 1..index].iter().collect();
                    self.apply_sgr(&parameters);
                }
                index + 1
            }
            ']' | 'P' | 'X' | '^' | '_' => {
                // OSC, DCS, SOS, PM, APC: a string ended by BEL or ESC \.
                let mut index = start + 1;
                while index < chars.len() {
                    if chars[index] == '\u{07}' {
                        return index + 1;
                    }
                    if chars[index] == '\u{1B}' {
                        return (index + 2).min(chars.len());
                    }
                    index += 1;
                }
                index
            }
            // Character set designation: one more byte.
            '(' | ')' | '*' | '+' => (start + 2).min(chars.len()),
            _ => start + 1,
        }
    }

    fn append(&mut self, c: char) {
        match self.spans.last_mut() {
            Some(span) if span.style == self.style => span.text.push(c),
            _ => self.spans.push(Span { text: c.to_string(), style: self.style }),
        }
    }

    fn apply_sgr(&mut self, parameters: &str) {
        let codes: Vec<i64> = parameters.split(';').map(|code| code.parse().unwrap_or(0)).collect();
        let style = &mut self.style;
        let mut index = 0;
        while index < codes.len() {
            let code = codes[index];
            match code {
                0 => *style = TextStyle::default(),
                1 => style.is_bold = true,
                2 => style.is_dim = true,
                3 => style.is_italic = true,
                4 => style.is_underlined = true,
                7 => style.is_inverse = true,
                9 => style.is_strikethrough = true,
                22 => (style.is_bold, style.is_dim) = (false, false),
                23 => style.is_italic = false,
                24 => style.is_underlined = false,
                27 => style.is_inverse = false,
                29 => style.is_strikethrough = false,
                30..=37 => style.foreground = Some(AnsiColor::Indexed { index: (code - 30) as u8 }),
                39 => style.foreground = None,
                40..=47 => style.background = Some(AnsiColor::Indexed { index: (code - 40) as u8 }),
                49 => style.background = None,
                90..=97 => style.foreground = Some(AnsiColor::Indexed { index: (code - 90 + 8) as u8 }),
                100..=107 => style.background = Some(AnsiColor::Indexed { index: (code - 100 + 8) as u8 }),
                38 | 48 => {
                    let (color, consumed) = extended_color(&codes[index + 1..]);
                    if let Some(color) = color {
                        if code == 38 {
                            style.foreground = Some(color);
                        } else {
                            style.background = Some(color);
                        }
                    }
                    index += consumed;
                }
                _ => {}
            }
            index += 1;
        }
    }
}

/// Reads `5;n` or `2;r;g;b`; returns the colour (if valid) and how many codes it used.
fn extended_color(codes: &[i64]) -> (Option<AnsiColor>, usize) {
    let byte = |index: usize| codes.get(index).and_then(|&code| u8::try_from(code).ok());
    match codes.first() {
        Some(5) => match byte(1) {
            Some(index) => (Some(AnsiColor::Indexed { index }), 2),
            None => (None, codes.len()),
        },
        Some(2) => match (byte(1), byte(2), byte(3)) {
            (Some(red), Some(green), Some(blue)) => (Some(AnsiColor::Rgb { red, green, blue }), 4),
            _ => (None, codes.len()),
        },
        _ => (None, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESC: &str = "\u{1B}";

    fn indexed(index: u8) -> Option<AnsiColor> {
        Some(AnsiColor::Indexed { index })
    }

    fn span(text: &str, style: TextStyle) -> Span {
        Span { text: text.into(), style }
    }

    #[test]
    fn plain_text_is_one_unstyled_span() {
        let line = Ansi::parse("hello world");
        assert_eq!(line.spans, [span("hello world", TextStyle::default())]);
        assert_eq!(line.plain_text(), "hello world");
    }

    #[test]
    fn an_empty_line_has_no_spans() {
        assert!(Ansi::parse("").spans.is_empty());
    }

    #[test]
    fn standard_and_bright_foreground_colors() {
        let line = Ansi::parse(&format!("{ESC}[31mred{ESC}[0m {ESC}[92mbright green{ESC}[39mdefault"));
        assert_eq!(
            line.spans,
            [
                span("red", TextStyle { foreground: indexed(1), ..Default::default() }),
                span(" ", TextStyle::default()),
                span("bright green", TextStyle { foreground: indexed(10), ..Default::default() }),
                span("default", TextStyle::default()),
            ]
        );
    }

    #[test]
    fn background_colors() {
        let line = Ansi::parse(&format!("{ESC}[44mblue{ESC}[103myellow{ESC}[49mnone"));
        let backgrounds: Vec<_> = line.spans.iter().map(|span| span.style.background).collect();
        assert_eq!(backgrounds, [indexed(4), indexed(11), None]);
    }

    #[test]
    fn colors_256_and_truecolor() {
        let line = Ansi::parse(&format!("{ESC}[38;5;208morange{ESC}[48;2;10;20;30mrgb"));
        assert_eq!(line.spans[0].style.foreground, indexed(208));
        assert_eq!(line.spans[1].style.foreground, indexed(208));
        assert_eq!(line.spans[1].style.background, Some(AnsiColor::Rgb { red: 10, green: 20, blue: 30 }));
    }

    #[test]
    fn combined_parameters_and_attribute_toggles() {
        let line = Ansi::parse(&format!("{ESC}[1;4;31mA{ESC}[22mB{ESC}[24;2;3mC{ESC}[mD"));
        let red = indexed(1);
        assert_eq!(
            line.spans[0].style,
            TextStyle { foreground: red, is_bold: true, is_underlined: true, ..Default::default() }
        );
        assert_eq!(line.spans[1].style, TextStyle { foreground: red, is_underlined: true, ..Default::default() });
        assert_eq!(
            line.spans[2].style,
            TextStyle { foreground: red, is_dim: true, is_italic: true, ..Default::default() }
        );
        assert_eq!(line.spans[3].style, TextStyle::default());
    }

    #[test]
    fn inverse_and_strikethrough() {
        let line = Ansi::parse(&format!("{ESC}[7;9mX{ESC}[27;29mY"));
        assert_eq!(line.spans[0].style, TextStyle { is_inverse: true, is_strikethrough: true, ..Default::default() });
        assert_eq!(line.spans[1].style, TextStyle::default());
    }

    #[test]
    fn adjacent_text_with_the_same_style_is_merged() {
        let line = Ansi::parse(&format!("{ESC}[31ma{ESC}[31mb{ESC}[1m{ESC}[22mc"));
        assert_eq!(line.spans, [span("abc", TextStyle { foreground: indexed(1), ..Default::default() })]);
    }

    #[test]
    fn strips_non_sgr_control_sequences() {
        assert_eq!(Ansi::parse(&format!("{ESC}[2K{ESC}[1Gdone{ESC}[?25h {ESC}(Bok{ESC}7")).plain_text(), "done ok");
    }

    #[test]
    fn keeps_the_text_of_osc_8_hyperlinks_and_drops_the_rest_of_osc() {
        let link = format!("{ESC}]8;;http://localhost:3000{ESC}\\http://localhost:3000{ESC}]8;;{ESC}\\");
        let title = format!("{ESC}]0;window title\u{07}");
        assert_eq!(Ansi::parse(&format!("{title}Local: {link}")).plain_text(), "Local: http://localhost:3000");
    }

    #[test]
    fn a_carriage_return_keeps_only_what_was_written_after_it() {
        assert_eq!(
            Ansi::parse(&format!("10%\r20%\r{ESC}[32m100%")).spans,
            [span("100%", TextStyle { foreground: indexed(2), ..Default::default() })]
        );
    }

    #[test]
    fn drops_other_control_characters_but_keeps_tabs() {
        assert_eq!(Ansi::parse("a\tb\u{07}c\u{08}d\u{7F}").plain_text(), "a\tbcd");
    }

    #[test]
    fn an_unterminated_escape_at_the_end_of_the_line_is_dropped() {
        assert_eq!(Ansi::parse(&format!("text{ESC}[31")).plain_text(), "text");
        assert_eq!(Ansi::parse(&format!("text{ESC}")).plain_text(), "text");
    }

    #[test]
    fn malformed_extended_colors_are_ignored_without_breaking_what_follows() {
        let line = Ansi::parse(&format!("{ESC}[38;5mA{ESC}[38;2;1;2mB{ESC}[1mC"));
        assert_eq!(line.plain_text(), "ABC");
        assert!(line.spans.last().unwrap().style.is_bold);
    }

    #[test]
    fn strip_returns_the_plain_text() {
        assert_eq!(Ansi::strip(&format!("{ESC}[1;31merror{ESC}[0m: boom")), "error: boom");
    }
}
