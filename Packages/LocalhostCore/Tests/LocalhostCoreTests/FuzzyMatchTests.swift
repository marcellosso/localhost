import Testing
@testable import LocalhostCore

struct FuzzyMatchTests {
    @Test func `an empty query matches anything`() {
        #expect(FuzzyMatch.score("", in: "web") == 0)
        #expect(FuzzyMatch.score("  ", in: "web") == 0)
    }

    @Test func `every word must be found as letters in order`() {
        #expect(FuzzyMatch.score("start web", in: "Start web · Acme") != nil)
        #expect(FuzzyMatch.score("stweb", in: "Start web") != nil)
        #expect(FuzzyMatch.score("web start", in: "Start web") != nil, "words match independently")
        #expect(FuzzyMatch.score("stop web", in: "Start web") == nil)
        #expect(FuzzyMatch.score("bew", in: "web") == nil)
    }

    @Test func `matching ignores case and accents`() {
        #expect(FuzzyMatch.score("CAFE", in: "Café API") != nil)
    }

    @Test func `prefixes and word starts beat letters scattered inside words`() throws {
        let prefix = try #require(FuzzyMatch.score("web", in: "web"))
        let wordStart = try #require(FuzzyMatch.score("web", in: "Open web"))
        let scattered = try #require(FuzzyMatch.score("web", in: "Sweden Bar"))
        #expect(prefix > wordStart)
        #expect(wordStart > scattered)
    }

    @Test func `the best occurrence is used, not the first`() throws {
        // The first "a" is inside "Data"; the word "api" later is a better match.
        let score = try #require(FuzzyMatch.score("api", in: "Data api"))
        let direct = try #require(FuzzyMatch.score("api", in: "Open api"))
        #expect(score == direct)
    }

    @Test func `initials of words match`() throws {
        let initials = try #require(FuzzyMatch.score("oib", in: "Open in Browser"))
        let inside = try #require(FuzzyMatch.score("oib", in: "Robin Bob"))
        #expect(initials > inside)
    }
}
