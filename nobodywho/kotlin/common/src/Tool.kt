package ai.nobodywho

import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import uniffi.nobodywho.RustTool as InternalRustTool
import uniffi.nobodywho.RustToolCallback
import uniffi.nobodywho.ToolParameter
import kotlin.reflect.KFunction
import kotlin.reflect.KParameter
import kotlin.reflect.full.callSuspendBy
import kotlin.reflect.full.valueParameters

/**
 * A tool that the model can call during inference.
 *
 * Create a tool by passing a function reference. Parameter names, types, and
 * JSON parsing are handled automatically via Kotlin reflection.
 *
 * Both regular and `suspend` functions are supported. Suspend functions are executed
 * via `runBlocking` on the Rust inference worker thread — this does **not** block the
 * main thread or any coroutine dispatcher.
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
 * Suspend functions work the same way:
 *
 * ```kotlin
 * suspend fun fetchData(query: String): String {
 *     return httpClient.get("https://api.example.com?q=$query").bodyAsText()
 * }
 *
 * val fetchTool = Tool(
 *     name = "fetch_data",
 *     description = "Fetch data from an API",
 *     function = ::fetchData
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
    function: KFunction<*>
) {
    internal val inner: InternalRustTool

    init {
        require(function.returnType.classifier == String::class) {
            "Tool function must return String, but returns ${function.returnType}"
        }

        val params = function.valueParameters
        val isSuspend = function.isSuspend

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
                // Tool callbacks are invoked on the Rust inference worker thread
                // (a dedicated std::thread, not the main thread or any coroutine
                // dispatcher), so runBlocking is safe here.
                val result = if (isSuspend) {
                    runBlocking { function.callSuspendBy(argMap) }
                } else {
                    function.callBy(argMap)
                }
                return result as String
            }
        }

        inner = InternalRustTool(name, description, toolParams, callback)
    }

    /** Get the JSON schema for this tool's parameters. */
    fun getSchemaJson(): String = inner.getSchemaJson()
}
