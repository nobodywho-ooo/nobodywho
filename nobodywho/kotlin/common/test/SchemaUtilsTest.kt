package ai.nobodywho

import org.json.JSONArray
import org.json.JSONObject
import org.junit.Assert.*
import org.junit.Test
import kotlin.reflect.typeOf

class SchemaUtilsTest {

    // ── jsonSchema ──

    @Test fun `String produces string schema`() =
        assertEquals("""{"type": "string"}""", jsonSchema(typeOf<String>()))
    @Test fun `Int produces integer schema`() =
        assertEquals("""{"type": "integer"}""", jsonSchema(typeOf<Int>()))
    @Test fun `Long produces integer schema`() =
        assertEquals("""{"type": "integer"}""", jsonSchema(typeOf<Long>()))
    @Test fun `Double produces number schema`() =
        assertEquals("""{"type": "number"}""", jsonSchema(typeOf<Double>()))
    @Test fun `Float produces number schema`() =
        assertEquals("""{"type": "number"}""", jsonSchema(typeOf<Float>()))
    @Test fun `Boolean produces boolean schema`() =
        assertEquals("""{"type": "boolean"}""", jsonSchema(typeOf<Boolean>()))
    @Test fun `Any falls back to string`() =
        assertEquals("""{"type": "string"}""", jsonSchema(typeOf<Any>()))

    @Test fun `List of Int produces array schema`() {
        val s = JSONObject(jsonSchema(typeOf<List<Int>>()))
        assertEquals("array", s.getString("type"))
        assertEquals("integer", s.getJSONObject("items").getString("type"))
    }

    @Test fun `Map of String to Float produces object schema`() {
        val s = JSONObject(jsonSchema(typeOf<Map<String, Float>>()))
        assertEquals("object", s.getString("type"))
        assertEquals("number", s.getJSONObject("additionalProperties").getString("type"))
    }

    @Test fun `nested List of List of Int`() {
        val s = JSONObject(jsonSchema(typeOf<List<List<Int>>>()))
        assertEquals("array", s.getJSONObject("items").getString("type"))
        assertEquals("integer", s.getJSONObject("items").getJSONObject("items").getString("type"))
    }

    @Test fun `nested Map of String to List of String`() {
        val s = JSONObject(jsonSchema(typeOf<Map<String, List<String>>>()))
        assertEquals("array", s.getJSONObject("additionalProperties").getString("type"))
    }

    // ── convertValue ──

    private fun schema(type: String) = JSONObject("""{"type": "$type"}""")

    @Test fun `null passthrough`() =
        assertNull(convertValue(null, schema("string")))
    @Test fun `JSONObject NULL passthrough`() =
        assertNull(convertValue(JSONObject.NULL, schema("string")))

    @Test fun `string identity`() =
        assertEquals("hello", convertValue("hello", schema("string")))
    @Test fun `int to string coercion`() =
        assertEquals("42", convertValue(42, schema("string")))

    @Test fun `int identity`() =
        assertEquals(42, convertValue(42, schema("integer")))
    @Test fun `long to int`() =
        assertEquals(42, convertValue(42L, schema("integer")))
    @Test fun `double to int truncate`() =
        assertEquals(42, convertValue(42.7, schema("integer")))
    @Test fun `string to int parse`() =
        assertEquals(42, convertValue("42", schema("integer")))
    @Test fun `bad string to int zero`() =
        assertEquals(0, convertValue("bad", schema("integer")))

    @Test fun `double identity`() =
        assertEquals(3.14, convertValue(3.14, schema("number")))
    @Test fun `int to double`() =
        assertEquals(42.0, convertValue(42, schema("number")))

    @Test fun `bool identity`() =
        assertEquals(true, convertValue(true, schema("boolean")))
    @Test fun `string true to bool`() =
        assertEquals(true, convertValue("true", schema("boolean")))
    @Test fun `non-bool to false`() =
        assertEquals(false, convertValue(42, schema("boolean")))

    @Test fun `JSONArray to list`() {
        val arrSchema = JSONObject("""{"type": "array", "items": {"type": "integer"}}""")
        assertEquals(listOf(1, 2, 3), convertValue(JSONArray("[1, 2, 3]"), arrSchema))
    }
    @Test fun `empty array`() {
        val arrSchema = JSONObject("""{"type": "array", "items": {"type": "integer"}}""")
        assertEquals(emptyList<Int>(), convertValue(JSONArray("[]"), arrSchema))
    }
    @Test fun `scalar wrapped in list`() {
        val arrSchema = JSONObject("""{"type": "array", "items": {"type": "integer"}}""")
        assertEquals(listOf(42), convertValue(42, arrSchema))
    }

    @Test fun `nested array`() {
        val nestedArr = JSONObject("""{"type": "array", "items": {"type": "array", "items": {"type": "string"}}}""")
        assertEquals(
            listOf(listOf("a", "b"), listOf("c")),
            convertValue(JSONArray("""[["a","b"],["c"]]"""), nestedArr)
        )
    }

    @Test fun `object values`() {
        val objSchema = JSONObject("""{"type": "object", "additionalProperties": {"type": "integer"}}""")
        val obj = convertValue(JSONObject("""{"x": 1, "y": 2}"""), objSchema) as Map<*, *>
        assertEquals(1, obj["x"])
        assertEquals(2, obj["y"])
    }
    @Test fun `non-object to empty map`() {
        val objSchema = JSONObject("""{"type": "object", "additionalProperties": {"type": "integer"}}""")
        assertEquals(mapOf<String, Any?>(), convertValue("bad", objSchema))
    }

    @Test fun `object with array values`() {
        val objArr = JSONObject("""{"type": "object", "additionalProperties": {"type": "array", "items": {"type": "string"}}}""")
        val r = convertValue(JSONObject("""{"tags": ["a","b"]}"""), objArr) as Map<*, *>
        assertEquals(listOf("a", "b"), r["tags"])
    }
}
