import Testing
@testable import LocalhostCore

struct ANSITests {
    let esc = "\u{1B}"

    func spans(_ line: String) -> [(String, TextStyle)] {
        ANSI.parse(line).spans.map { ($0.text, $0.style) }
    }

    @Test func `plain text is one unstyled span`() {
        let line = ANSI.parse("hello world")
        #expect(line.spans == [StyledLine.Span(text: "hello world", style: TextStyle())])
        #expect(line.plainText == "hello world")
    }

    @Test func `an empty line has no spans`() {
        #expect(ANSI.parse("").spans.isEmpty)
    }

    @Test func `standard and bright foreground colors`() {
        let line = ANSI.parse("\(esc)[31mred\(esc)[0m \(esc)[92mbright green\(esc)[39mdefault")
        #expect(line.spans == [
            .init(text: "red", style: TextStyle(foreground: .indexed(1))),
            .init(text: " ", style: TextStyle()),
            .init(text: "bright green", style: TextStyle(foreground: .indexed(10))),
            .init(text: "default", style: TextStyle()),
        ])
    }

    @Test func `background colors`() {
        let line = ANSI.parse("\(esc)[44mblue\(esc)[103myellow\(esc)[49mnone")
        #expect(line.spans.map(\.style.background) == [.indexed(4), .indexed(11), nil])
    }

    @Test func `256-color and truecolor`() {
        let line = ANSI.parse("\(esc)[38;5;208morange\(esc)[48;2;10;20;30mrgb")
        #expect(line.spans[0].style.foreground == .indexed(208))
        #expect(line.spans[1].style.foreground == .indexed(208))
        #expect(line.spans[1].style.background == .rgb(red: 10, green: 20, blue: 30))
    }

    @Test func `combined parameters and attribute toggles`() {
        let line = ANSI.parse("\(esc)[1;4;31mA\(esc)[22mB\(esc)[24;2;3mC\(esc)[mD")
        #expect(line.spans[0].style == TextStyle(foreground: .indexed(1), isBold: true, isUnderlined: true))
        #expect(line.spans[1].style == TextStyle(foreground: .indexed(1), isUnderlined: true))
        #expect(line.spans[2].style == TextStyle(foreground: .indexed(1), isDim: true, isItalic: true))
        #expect(line.spans[3].style == TextStyle())
    }

    @Test func `inverse and strikethrough`() {
        let line = ANSI.parse("\(esc)[7;9mX\(esc)[27;29mY")
        #expect(line.spans[0].style == TextStyle(isInverse: true, isStrikethrough: true))
        #expect(line.spans[1].style == TextStyle())
    }

    @Test func `adjacent text with the same style is merged`() {
        let line = ANSI.parse("\(esc)[31ma\(esc)[31mb\(esc)[1m\(esc)[22mc")
        #expect(line.spans == [.init(text: "abc", style: TextStyle(foreground: .indexed(1)))])
    }

    @Test func `strips non-SGR control sequences`() {
        let line = ANSI.parse("\(esc)[2K\(esc)[1Gdone\(esc)[?25h \(esc)(Bok\(esc)7")
        #expect(line.plainText == "done ok")
    }

    @Test func `keeps the text of OSC 8 hyperlinks and drops the rest of OSC`() {
        let link = "\(esc)]8;;http://localhost:3000\(esc)\\http://localhost:3000\(esc)]8;;\(esc)\\"
        let title = "\(esc)]0;window title\u{07}"
        #expect(ANSI.parse("\(title)Local: \(link)").plainText == "Local: http://localhost:3000")
    }

    @Test func `a carriage return keeps only what was written after it`() {
        #expect(ANSI.parse("10%\r20%\r\(esc)[32m100%").spans == [
            .init(text: "100%", style: TextStyle(foreground: .indexed(2))),
        ])
    }

    @Test func `drops other control characters but keeps tabs`() {
        #expect(ANSI.parse("a\tb\u{07}c\u{08}d\u{7F}").plainText == "a\tbcd")
    }

    @Test func `an unterminated escape at the end of the line is dropped`() {
        #expect(ANSI.parse("text\(esc)[31").plainText == "text")
        #expect(ANSI.parse("text\(esc)").plainText == "text")
    }

    @Test func `malformed extended colors are ignored without breaking what follows`() {
        let line = ANSI.parse("\(esc)[38;5mA\(esc)[38;2;1;2mB\(esc)[1mC")
        #expect(line.plainText == "ABC")
        #expect(line.spans.last?.style.isBold == true)
    }

    @Test func `strip returns the plain text`() {
        #expect(ANSI.strip("\(esc)[1;31merror\(esc)[0m: boom") == "error: boom")
    }
}
