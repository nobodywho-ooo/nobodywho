import SwiftSyntax
import SwiftSyntaxMacros
import SwiftCompilerPlugin

/// Maps Swift type names to JSON Schema type strings.
private func jsonSchemaType(for swiftType: String) -> String {
    switch swiftType {
    case "String":
        return "string"
    case "Int", "Int8", "Int16", "Int32", "Int64",
         "UInt", "UInt8", "UInt16", "UInt32", "UInt64":
        return "integer"
    case "Double", "Float", "Float32", "Float64", "CGFloat":
        return "number"
    case "Bool":
        return "boolean"
    default:
        return "string"
    }
}

/// Generates a cast expression to extract a typed value from parsed JSON.
private func castExpression(name: String, schemaType: String) -> String {
    switch schemaType {
    case "integer":
        return "(parsed[\"\(name)\"] as? NSNumber)?.intValue ?? 0"
    case "number":
        return "(parsed[\"\(name)\"] as? NSNumber)?.doubleValue ?? 0.0"
    case "boolean":
        return "(parsed[\"\(name)\"] as? Bool) ?? false"
    default:
        return "(parsed[\"\(name)\"] as? String) ?? \"\""
    }
}

public struct ToolMacro: PeerMacro {
    public static func expansion(
        of node: AttributeSyntax,
        providingPeersOf declaration: some DeclSyntaxProtocol,
        in context: some MacroExpansionContext
    ) throws -> [DeclSyntax] {
        guard let funcDecl = declaration.as(FunctionDeclSyntax.self) else {
            throw ToolMacroError.onlyApplicableToFunction
        }

        // Extract description from @Tool("...")
        guard let arguments = node.arguments?.as(LabeledExprListSyntax.self),
              let firstArg = arguments.first,
              let descriptionLiteral = firstArg.expression.as(StringLiteralExprSyntax.self),
              let descriptionSegment = descriptionLiteral.segments.first?.as(StringSegmentSyntax.self) else {
            throw ToolMacroError.missingDescription
        }
        let description = descriptionSegment.content.text

        let funcName = funcDecl.name.text
        let parameters = funcDecl.signature.parameterClause.parameters

        // Build ToolParameter array entries and argument extraction lines
        var toolParamEntries: [String] = []
        var argLetBindings: [String] = []

        for param in parameters {
            let internalName = param.secondName?.text ?? param.firstName.text
            let typeText = param.type.description.trimmingCharacters(in: .whitespaces)
            let schemaType = jsonSchemaType(for: typeText)

            toolParamEntries.append(
                "ToolParameter(name: \"\(internalName)\", schema: #\"{\"type\": \"\(schemaType)\"}\"#)"
            )
            argLetBindings.append(
                "let \(internalName) = \(castExpression(name: internalName, schemaType: schemaType))"
            )
        }

        // Build the function call expression with named arguments
        let callArgs = parameters.map { param in
            let internalName = param.secondName?.text ?? param.firstName.text
            let externalName = param.firstName.text
            if externalName == "_" {
                return internalName
            }
            return "\(externalName): \(internalName)"
        }.joined(separator: ", ")

        let paramListCode: String
        if toolParamEntries.isEmpty {
            paramListCode = "[]"
        } else {
            paramListCode = "[\n            \(toolParamEntries.joined(separator: ",\n            "))\n        ]"
        }

        let argBindingsCode = argLetBindings.joined(separator: "\n            ")

        let generated = """
        let \(funcName)Tool = Tool(
            name: "\(funcName)",
            description: "\(description)",
            parameters: \(paramListCode),
            callback: ToolCallbackClosure { argumentsJson in
                guard let data = argumentsJson.data(using: .utf8),
                      let parsed = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    return "Error: Failed to parse arguments"
                }
                \(argBindingsCode)
                return \(funcName)(\(callArgs))
            }
        )
        """

        return [DeclSyntax(stringLiteral: generated)]
    }
}

enum ToolMacroError: Error, CustomStringConvertible {
    case onlyApplicableToFunction
    case missingDescription

    var description: String {
        switch self {
        case .onlyApplicableToFunction:
            return "@Tool can only be applied to a function"
        case .missingDescription:
            return "@Tool requires a description string argument, e.g. @Tool(\"Describes what the tool does\")"
        }
    }
}

@main
struct NobodyWhoMacrosPlugin: CompilerPlugin {
    let providingMacros: [Macro.Type] = [
        ToolMacro.self,
    ]
}
