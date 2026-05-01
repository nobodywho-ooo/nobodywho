import SwiftSyntax
import SwiftSyntaxMacros
import SwiftCompilerPlugin

/// Recursively generates a JSON Schema string from a Swift type syntax node.
private func jsonSchema(for type: TypeSyntax) -> String {
    // [T] shorthand syntax
    if let array = type.as(ArrayTypeSyntax.self) {
        let itemSchema = jsonSchema(for: array.element)
        return "{\"type\": \"array\", \"items\": \(itemSchema)}"
    }

    // [K: V] shorthand syntax
    if let dict = type.as(DictionaryTypeSyntax.self) {
        let valueSchema = jsonSchema(for: dict.value)
        return "{\"type\": \"object\", \"additionalProperties\": \(valueSchema)}"
    }

    // Named types: primitives, Array<T>, Dictionary<K, V>
    if let ident = type.as(IdentifierTypeSyntax.self) {
        let name = ident.name.text

        if name == "Array", let args = ident.genericArgumentClause {
            let itemSchema = jsonSchema(for: args.arguments.first!.argument)
            return "{\"type\": \"array\", \"items\": \(itemSchema)}"
        }

        if name == "Dictionary", let args = ident.genericArgumentClause {
            let argsArray = Array(args.arguments)
            let valueSchema = jsonSchema(for: argsArray[1].argument)
            return "{\"type\": \"object\", \"additionalProperties\": \(valueSchema)}"
        }

        return primitiveSchema(for: name)
    }

    return "{\"type\": \"string\"}"
}

/// Maps a Swift primitive type name to a JSON Schema string.
private func primitiveSchema(for name: String) -> String {
    switch name {
    case "String":
        return "{\"type\": \"string\"}"
    case "Int", "Int8", "Int16", "Int32", "Int64",
         "UInt", "UInt8", "UInt16", "UInt32", "UInt64":
        return "{\"type\": \"integer\"}"
    case "Double", "Float", "Float32", "Float64", "CGFloat":
        return "{\"type\": \"number\"}"
    case "Bool":
        return "{\"type\": \"boolean\"}"
    default:
        return "{\"type\": \"string\"}"
    }
}

/// Recursively generates a cast expression to convert a value from `[Any]`/`[String: Any]`
/// back to the expected Swift type. `expr` is the expression to cast (e.g. "args[0]" or "$0").
private func castExpression(for type: TypeSyntax, from expr: String) -> String {
    // [T]
    if let array = type.as(ArrayTypeSyntax.self) {
        let inner = castExpression(for: array.element, from: "$0")
        return "(\(expr) as! [Any]).map { \(inner) }"
    }

    // [K: V]
    if let dict = type.as(DictionaryTypeSyntax.self) {
        let inner = castExpression(for: dict.value, from: "$0")
        return "(\(expr) as! [String: Any]).mapValues { \(inner) }"
    }

    // Named types
    if let ident = type.as(IdentifierTypeSyntax.self) {
        let name = ident.name.text

        if name == "Array", let args = ident.genericArgumentClause {
            let inner = castExpression(for: args.arguments.first!.argument, from: "$0")
            return "(\(expr) as! [Any]).map { \(inner) }"
        }

        if name == "Dictionary", let args = ident.genericArgumentClause {
            let argsArray = Array(args.arguments)
            let inner = castExpression(for: argsArray[1].argument, from: "$0")
            return "(\(expr) as! [String: Any]).mapValues { \(inner) }"
        }

        return primitiveCast(for: name, from: expr)
    }

    return "\(expr) as! String"
}

/// Generates a cast expression for a primitive Swift type.
private func primitiveCast(for name: String, from expr: String) -> String {
    switch name {
    case "String":
        return "\(expr) as! String"
    case "Int":
        return "\(expr) as! Int"
    case "Double", "Float64":
        return "\(expr) as! Double"
    case "Bool":
        return "\(expr) as! Bool"
    case "Float", "Float32":
        return "Float(\(expr) as! Double)"
    case "CGFloat":
        return "CGFloat(\(expr) as! Double)"
    case "Int8":
        return "Int8(\(expr) as! Int)"
    case "Int16":
        return "Int16(\(expr) as! Int)"
    case "Int32":
        return "Int32(\(expr) as! Int)"
    case "Int64":
        return "Int64(\(expr) as! Int)"
    case "UInt":
        return "UInt(\(expr) as! Int)"
    case "UInt8":
        return "UInt8(\(expr) as! Int)"
    case "UInt16":
        return "UInt16(\(expr) as! Int)"
    case "UInt32":
        return "UInt32(\(expr) as! Int)"
    case "UInt64":
        return "UInt64(\(expr) as! Int)"
    default:
        return "\(expr) as! String"
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
            let type = param.type
            let schema = jsonSchema(for: type)

            let escapedSchema = schema.replacingOccurrences(of: "\\", with: "\\\\")
                                      .replacingOccurrences(of: "\"", with: "\\\"")
            paramTuples.append("(\"\(internalName)\", \"\(escapedSchema)\")")
            argLetBindings.append("let \(internalName) = \(castExpression(for: type, from: "args[\(index)]"))")
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
