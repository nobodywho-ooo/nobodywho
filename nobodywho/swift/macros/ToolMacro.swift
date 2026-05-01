import SwiftSyntax
import SwiftSyntaxMacros
import SwiftCompilerPlugin

/// Maps Swift type names to the parameter type strings used by Tool's tuple API.
private func paramType(for swiftType: String) -> String {
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

/// Generates a cast expression to extract a typed value from the args array.
/// The args array contains values pre-converted by Tool's internal parseArgs:
///   "string" → String, "integer" → Int, "number" → Double, "boolean" → Bool
private func argCast(index: Int, swiftType: String) -> String {
    switch swiftType {
    case "Float", "Float32":
        return "Float(args[\(index)] as! Double)"
    case "CGFloat":
        return "CGFloat(args[\(index)] as! Double)"
    case "Int8":
        return "Int8(args[\(index)] as! Int)"
    case "Int16":
        return "Int16(args[\(index)] as! Int)"
    case "Int32":
        return "Int32(args[\(index)] as! Int)"
    case "Int64":
        return "Int64(args[\(index)] as! Int)"
    case "UInt":
        return "UInt(args[\(index)] as! Int)"
    case "UInt8":
        return "UInt8(args[\(index)] as! Int)"
    case "UInt16":
        return "UInt16(args[\(index)] as! Int)"
    case "UInt32":
        return "UInt32(args[\(index)] as! Int)"
    case "UInt64":
        return "UInt64(args[\(index)] as! Int)"
    case "Int":
        return "args[\(index)] as! Int"
    case "Double", "Float64":
        return "args[\(index)] as! Double"
    case "Bool":
        return "args[\(index)] as! Bool"
    default:
        return "args[\(index)] as! String"
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

        // Extract description from @DeclareTool("...")
        guard let arguments = node.arguments?.as(LabeledExprListSyntax.self),
              let firstArg = arguments.first,
              let descriptionLiteral = firstArg.expression.as(StringLiteralExprSyntax.self),
              let descriptionSegment = descriptionLiteral.segments.first?.as(StringSegmentSyntax.self) else {
            throw ToolMacroError.missingDescription
        }
        let description = descriptionSegment.content.text

        let funcName = funcDecl.name.text
        let parameters = funcDecl.signature.parameterClause.parameters
        let isAsync = funcDecl.signature.effectSpecifiers?.asyncSpecifier != nil

        // Build parameter tuples and argument extraction lines
        var paramTuples: [String] = []
        var argLetBindings: [String] = []

        for (index, param) in parameters.enumerated() {
            let internalName = param.secondName?.text ?? param.firstName.text
            let typeText = param.type.description.trimmingCharacters(in: .whitespaces)

            paramTuples.append("(\"\(internalName)\", \"\(paramType(for: typeText))\")")
            argLetBindings.append("let \(internalName) = \(argCast(index: index, swiftType: typeText))")
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
        if paramTuples.isEmpty {
            paramListCode = "[]"
        } else {
            paramListCode = "[\(paramTuples.joined(separator: ", "))]"
        }

        let callExpr = isAsync
            ? "await \(funcName)(\(callArgs))"
            : "\(funcName)(\(callArgs))"

        let closureParam = parameters.isEmpty ? "_ in" : "args in"
        let argBindingsCode = argLetBindings.map { "    \($0)" }.joined(separator: "\n")

        let closureBody: String
        if argLetBindings.isEmpty {
            closureBody = "    return \(callExpr)"
        } else {
            closureBody = "\(argBindingsCode)\n    return \(callExpr)"
        }

        let generated = """
        let \(funcName)Tool = Tool(
            name: "\(funcName)",
            description: "\(description)",
            parameters: \(paramListCode)
        ) { \(closureParam)
        \(closureBody)
        }
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
