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
        List::class, MutableList::class, Collection::class -> {
            val itemType = type.arguments.firstOrNull()?.type
            val itemSchema = if (itemType != null) jsonSchema(itemType) else """{"type": "string"}"""
            """{"type": "array", "items": $itemSchema}"""
        }
        Map::class, MutableMap::class -> {
            val valueType = type.arguments.getOrNull(1)?.type
            val valueSchema = if (valueType != null) jsonSchema(valueType) else """{"type": "string"}"""
            """{"type": "object", "additionalProperties": $valueSchema}"""
        }
        else -> """{"type": "string"}"""
    }
}

/** Recursively converts a JSON value to a Kotlin type based on a JSON schema. */
internal fun convertValue(value: Any?, schema: JSONObject): Any? {
    if (value == null || value == JSONObject.NULL) return null
    return when (schema.optString("type")) {
        "string" -> value.toString()
        "integer" -> when (value) {
            is Number -> value.toInt()
            is String -> value.toIntOrNull() ?: 0
            else -> 0
        }
        "number" -> when (value) {
            is Number -> value.toDouble()
            is String -> value.toDoubleOrNull() ?: 0.0
            else -> 0.0
        }
        "boolean" -> when (value) {
            is Boolean -> value
            is String -> value == "true"
            else -> false
        }
        "array" -> {
            val itemSchema = schema.optJSONObject("items") ?: JSONObject("""{"type": "string"}""")
            when (value) {
                is JSONArray -> (0 until value.length()).map { i ->
                    convertValue(value.get(i), itemSchema)
                }
                else -> listOf(convertValue(value, itemSchema))
            }
        }
        "object" -> {
            val valueSchema = schema.optJSONObject("additionalProperties")
                ?: JSONObject("""{"type": "string"}""")
            when (value) {
                is JSONObject -> {
                    val map = mutableMapOf<String, Any?>()
                    for (key in value.keys()) {
                        map[key] = convertValue(value.get(key), valueSchema)
                    }
                    map
                }
                else -> mapOf<String, Any?>()
            }
        }
        else -> value.toString()
    }
}
