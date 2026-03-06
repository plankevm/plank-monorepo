import XCTest
import SwiftTreeSitter
import TreeSitterPlank

final class TreeSitterPlankTests: XCTestCase {
    func testCanLoadGrammar() throws {
        let parser = Parser()
        let language = Language(language: tree_sitter_plank())
        XCTAssertNoThrow(try parser.setLanguage(language),
                         "Error loading Plank grammar")
    }
}
