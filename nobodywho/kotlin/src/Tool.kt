package com.nobodywho

import org.json.JSONObject
import uniffi.nobodywho.RustTool as InternalRustTool
import uniffi.nobodywho.RustToolCallback
import uniffi.nobodywho.ToolParameter
import kotlin.reflect.KFunction
import kotlin.reflect.KParameter
import kotlin.reflect.full.valueParameters

/**
 * A tool that the model can call during inference.
 *
 * Create a tool by passing a function reference. Parameter names, types, and
 * JSON parsing are handled automatically via Kotlin reflection.
 *
 * ```kotlin
 * fun getWeather(city: String, unit: String): String {
 *     return """{"temp": 22, "unit": "$unit"}"""
 * }
 *
 * val weatherTool = Tool(
 *     name = "get_weather",
 *     description = "Get the current weather for a city",
 *     function = ::getWeather
 * )
 * ```
 *
 * Supported parameter types: `String`, `Int`, `Long`, `Double`, `Float`, `Boolean`,
 * and collections like `List<String>` or `Map<String, Int>`.
 *
 * **Important:** The function must be a top-level function, a class method, or a
 * companion object method. Local functions (defined inside other functions or lambdas)
 * are not supported because Kotlin reflection cannot resolve their signatures after
 * coroutine or closure compilation transforms them.
 */
class Tool(
    name: String,
    description: String,
    function: KFunction<String>
) {
    internal val inner: InternalRustTool

    init {
        val params = function.valueParameters

        val schemas = params.map { param -> jsonSchema(param.type) }

        val toolParams = params.zip(schemas).map { (param, schema) ->
            val paramName = param.name ?: throw IllegalArgumentException(
                "Tool function parameters must have names"
            )
            ToolParameter(name = paramName, schema = schema)
        }

        val parsedSchemas = schemas.map { JSONObject(it) }

        val callback = object : RustToolCallback {
            override fun call(argumentsJson: String): String {
                val parsed = JSONObject(argumentsJson)
                val argMap = mutableMapOf<KParameter, Any?>()
                for ((i, param) in params.withIndex()) {
                    val paramName = param.name!!
                    val rawValue = parsed.opt(paramName)
                    argMap[param] = convertValue(rawValue, parsedSchemas[i])
                }
                return function.callBy(argMap)
            }
        }

        inner = InternalRustTool(name, description, toolParams, callback)
    }

    /** Get the JSON schema for this tool's parameters. */
    fun getSchemaJson(): String = inner.getSchemaJson()
}
