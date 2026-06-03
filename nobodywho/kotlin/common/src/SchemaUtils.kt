package ai.nobodywho

import org.json.JSONArray
import org.json.JSONObject
import kotlin.reflect.KType

/** Recursively builds a JSON schema string from a Kotlin [KType]. */
internal fun jsonSchema(type: KType): String {
    val classifier = type.classifier
    return when (classifier) {
        String::class -> """{"type": "string"}"""
        Int::class, Long::class, Short::class, Byte::class -> """{"type": "integer"}"""
        Double::class, Float::class -> """{"type": "number"}"""
        Boolean::class -> """{"type": "boolean"}"""
        List::class -> {
            val itemType = type.arguments.firstOrNull()?.type
            val itemSchema = if (itemType != null) jsonSchema(itemType) else """{"type": "string"}"""
            """{"type": "array", "items": $itemSchema}"""
        }
        Map::class -> {
            val valueType = type.arguments.getOrNull(1)?.type
            val valueSchema = if (valueType != null) jsonSchema(valueType) else """{"type": "string"}"""
            """{"type": "object", "additionalProperties": $valueSchema}"""
        }
        else -> throw IllegalArgumentException(
            "Unsupported tool parameter type: ${type.classifier}. " +
            "Supported types: String, Int, Long, Short, Byte, Double, Float, Boolean, List<T>, Map<String, T>"
        )
    }
}

/** Recursively converts a JSON value to a Kotlin type based on a JSON schema. */
internal fun convertValue(value: Any?, schema: JSONObject): Any? {
    if (value == null || value == JSONObject.NULL) return null
    val type = schema.optString("type")
    return when (type) {
        "string" -> {
            if (value !is String) throw IllegalArgumentException("Expected String, got ${value::class.simpleName}")
            value
        }
        "integer" -> {
            if (value !is Number) throw IllegalArgumentException("Expected Number, got ${value::class.simpleName}")
            value.toInt()
        }
        "number" -> {
            if (value !is Number) throw IllegalArgumentException("Expected Number, got ${value::class.simpleName}")
            value.toDouble()
        }
        "boolean" -> {
            if (value !is Boolean) throw IllegalArgumentException("Expected Boolean, got ${value::class.simpleName}")
            value
        }
        "array" -> {
            if (value !is JSONArray) throw IllegalArgumentException("Expected JSONArray, got ${value::class.simpleName}")
            val itemSchema = schema.optJSONObject("items") ?: JSONObject("""{"type": "string"}""")
            (0 until value.length()).map { i ->
                convertValue(value.get(i), itemSchema)
            }
        }
        "object" -> {
            if (value !is JSONObject) throw IllegalArgumentException("Expected JSONObject, got ${value::class.simpleName}")
            val valueSchema = schema.optJSONObject("additionalProperties")
                ?: JSONObject("""{"type": "string"}""")
            val map = mutableMapOf<String, Any?>()
            for (key in value.keys()) {
                map[key] = convertValue(value.get(key), valueSchema)
            }
            map
        }
        else -> throw IllegalArgumentException("Unknown JSON schema type: '$type'")
    }
}
