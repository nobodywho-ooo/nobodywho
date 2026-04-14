package com.nobodywho

import org.json.JSONObject
import uniffi.nobodywho.RustTool as InternalRustTool
import uniffi.nobodywho.RustToolCallback
import uniffi.nobodywho.ToolParameter
import kotlin.reflect.KFunction
import kotlin.reflect.KParameter
import kotlin.reflect.KType
import kotlin.reflect.full.callSuspendBy
import kotlin.reflect.full.valueParameters

/**
 * Maps a Kotlin KType to a JSON Schema type string.
 */
private fun jsonSchemaType(type: KType): String {
    val classifier = type.classifier
    return when (classifier) {
        String::class -> "string"
        Int::class, Long::class, Short::class, Byte::class -> "integer"
        Double::class, Float::class -> "number"
        Boolean::class -> "boolean"
        else -> "string"
    }
}

/**
 * Converts a JSON value to the appropriate Kotlin type for a given KType.
 */
private fun convertValue(value: Any?, targetType: KType): Any? {
    if (value == null || value == JSONObject.NULL) return null
    val classifier = targetType.classifier
    return when (classifier) {
        String::class -> value.toString()
        Int::class -> when (value) {
            is Number -> value.toInt()
            is String -> value.toIntOrNull() ?: 0
            else -> 0
        }
        Long::class -> when (value) {
            is Number -> value.toLong()
            is String -> value.toLongOrNull() ?: 0L
            else -> 0L
        }
        Double::class -> when (value) {
            is Number -> value.toDouble()
            is String -> value.toDoubleOrNull() ?: 0.0
            else -> 0.0
        }
        Float::class -> when (value) {
            is Number -> value.toFloat()
            is String -> value.toFloatOrNull() ?: 0f
            else -> 0f
        }
        Boolean::class -> when (value) {
            is Boolean -> value
            is String -> value == "true"
            else -> false
        }
        else -> value.toString()
    }
}

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
 */
class Tool(
    name: String,
    description: String,
    function: KFunction<String>
) {
    internal val inner: InternalRustTool

    init {
        val params = function.valueParameters

        val toolParams = params.map { param ->
            val paramName = param.name ?: throw IllegalArgumentException(
                "Tool function parameters must have names"
            )
            val schemaType = jsonSchemaType(param.type)
            ToolParameter(name = paramName, schema = """{"type": "$schemaType"}""")
        }

        val callback = object : RustToolCallback {
            override fun call(argumentsJson: String): String {
                val parsed = JSONObject(argumentsJson)
                val argMap = mutableMapOf<KParameter, Any?>()
                for (param in params) {
                    val paramName = param.name!!
                    val rawValue = parsed.opt(paramName)
                    argMap[param] = convertValue(rawValue, param.type)
                }
                return function.callBy(argMap)
            }
        }

        inner = InternalRustTool(name, description, toolParams, callback)
    }

    /** Get the JSON schema for this tool's parameters. */
    fun getSchemaJson(): String = inner.getSchemaJson()
}
