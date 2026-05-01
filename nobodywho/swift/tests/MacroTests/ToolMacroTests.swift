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
                parameters: [("city", "string"), ("unit", "string")]
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
                parameters: [("query", "string")]
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
                parameters: [("level", "integer"), ("muted", "boolean")]
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

    #else
    func testMacrosUnavailable() throws {
        XCTSkip("Macros are only supported when building with the host compiler")
    }
    #endif
}
