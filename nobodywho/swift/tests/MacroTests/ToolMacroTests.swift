import SwiftSyntaxMacros
import SwiftSyntaxMacrosTestSupport
import XCTest

#if canImport(NobodyWhoMacros)
import NobodyWhoMacros

let testMacros: [String: Macro.Type] = [
    "DeclareTool": ToolMacro.self,
]
#endif

final class ToolMacroTests: XCTestCase {
    #if canImport(NobodyWhoMacros)

    func testSyncFunctionNoParams() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Get the current time")
            func getTime() -> String {
                return "12:00"
            }
            """,
            expandedSource: """
            func getTime() -> String {
                return "12:00"
            }

            let getTimeTool = Tool(
                name: "getTime",
                description: "Get the current time",
                parameters: []
            ) { _ in
                return getTime()
            }
            """,
            macros: testMacros
        )
    }

    func testSyncFunctionWithParams() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Get the weather")
            func getWeather(city: String, unit: String) -> String {
                return "sunny"
            }
            """,
            expandedSource: """
            func getWeather(city: String, unit: String) -> String {
                return "sunny"
            }

            let getWeatherTool = Tool(
                name: "getWeather",
                description: "Get the weather",
                parameters: [("city", #"{"type": "string"}"#), ("unit", #"{"type": "string"}"#)]
            ) { args in
                let city = args[0] as! String
                let unit = args[1] as! String
                return getWeather(city: city, unit: unit)
            }
            """,
            macros: testMacros
        )
    }

    func testAsyncFunction() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Search the database")
            func search(query: String) async -> String {
                return "results"
            }
            """,
            expandedSource: """
            func search(query: String) async -> String {
                return "results"
            }

            let searchTool = Tool(
                name: "search",
                description: "Search the database",
                parameters: [("query", #"{"type": "string"}"#)]
            ) { args in
                let query = args[0] as! String
                return await search(query: query)
            }
            """,
            macros: testMacros
        )
    }

    func testIntAndBoolParams() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Set volume")
            func setVolume(level: Int, muted: Bool) -> String {
                return "ok"
            }
            """,
            expandedSource: """
            func setVolume(level: Int, muted: Bool) -> String {
                return "ok"
            }

            let setVolumeTool = Tool(
                name: "setVolume",
                description: "Set volume",
                parameters: [("level", #"{"type": "integer"}"#), ("muted", #"{"type": "boolean"}"#)]
            ) { args in
                let level = args[0] as! Int
                let muted = args[1] as! Bool
                return setVolume(level: level, muted: muted)
            }
            """,
            macros: testMacros
        )
    }

    func testAsyncNoParams() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Ping the server")
            func ping() async -> String {
                return "pong"
            }
            """,
            expandedSource: """
            func ping() async -> String {
                return "pong"
            }

            let pingTool = Tool(
                name: "ping",
                description: "Ping the server",
                parameters: []
            ) { _ in
                return await ping()
            }
            """,
            macros: testMacros
        )
    }

    // MARK: - Array parameters

    func testArrayParam() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Process items")
            func process(items: [String]) -> String {
                return "done"
            }
            """,
            expandedSource: """
            func process(items: [String]) -> String {
                return "done"
            }

            let processTool = Tool(
                name: "process",
                description: "Process items",
                parameters: [("items", #"{"type": "array", "items": {"type": "string"}}"#)]
            ) { args in
                let items = (args[0] as! [Any]).map { $0 as! String }
                return process(items: items)
            }
            """,
            macros: testMacros
        )
    }

    // MARK: - Map parameters

    func testMapParam() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Store data")
            func store(data: [String: Int]) -> String {
                return "stored"
            }
            """,
            expandedSource: """
            func store(data: [String: Int]) -> String {
                return "stored"
            }

            let storeTool = Tool(
                name: "store",
                description: "Store data",
                parameters: [("data", #"{"type": "object", "additionalProperties": {"type": "integer"}}"#)]
            ) { args in
                let data = (args[0] as! [String: Any]).mapValues { $0 as! Int }
                return store(data: data)
            }
            """,
            macros: testMacros
        )
    }

    // MARK: - Nested parameters

    func testNestedArrayParam() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Process matrix")
            func process(matrix: [[Int]]) -> String {
                return "done"
            }
            """,
            expandedSource: """
            func process(matrix: [[Int]]) -> String {
                return "done"
            }

            let processTool = Tool(
                name: "process",
                description: "Process matrix",
                parameters: [("matrix", #"{"type": "array", "items": {"type": "array", "items": {"type": "integer"}}}"#)]
            ) { args in
                let matrix = (args[0] as! [Any]).map { ($0 as! [Any]).map { $0 as! Int } }
                return process(matrix: matrix)
            }
            """,
            macros: testMacros
        )
    }

    func testNestedMapParam() throws {
        assertMacroExpansion(
            """
            @DeclareTool("Update config")
            func update(config: [String: [String]]) -> String {
                return "updated"
            }
            """,
            expandedSource: """
            func update(config: [String: [String]]) -> String {
                return "updated"
            }

            let updateTool = Tool(
                name: "update",
                description: "Update config",
                parameters: [("config", #"{"type": "object", "additionalProperties": {"type": "array", "items": {"type": "string"}}}"#)]
            ) { args in
                let config = (args[0] as! [String: Any]).mapValues { ($0 as! [Any]).map { $0 as! String } }
                return update(config: config)
            }
            """,
            macros: testMacros
        )
    }

    #else
    func testMacrosUnavailable() throws {
        XCTSkip("Macros are only supported when building with the host compiler")
    }
    #endif
}
