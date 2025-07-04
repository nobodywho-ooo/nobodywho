using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using UnityEngine;

namespace NobodyWho
{
    // Example of a json schema:
    // {
    //     "function": {
    //       "type": "object",
    //       "properties": {
    //         "a": {"type": "integer", "description": "First number"},
    //         "b": {"type": "integer", "description": "Second number"}
    //     },
    //     "required": ["a", "b"]
    //   }
    // }

    public class ToolCall
    {
        public string name;
        public Delegate userDelegate;
        public string description;
        public string jsonSchema;
        public ToolCallback callback;

        private List<Delegate> _delegates = new List<Delegate>();

        public ToolCall(Delegate userDelegate, string description)
        {
            this.userDelegate = userDelegate;
            this.description = description;
            this.name = userDelegate.Method.Name;
            ParameterInfo[] parameterMeta = userDelegate.Method.GetParameters();
            this.callback = CreateTrampoline(userDelegate, parameterMeta);
            this.jsonSchema = BuildSchemaForDelegate(userDelegate);
            // Prevent garbage collection of delegates
            _delegates.Add(userDelegate);
            _delegates.Add(callback);
        }

        [System.Serializable]
        private class JsonSchema
        {
            public string type;
            public JsonProperties properties;
            public List<string> required;
        }

        [System.Serializable]
        private class JsonProperties
        {
            public List<JsonParameter> properties;
        }

        [System.Serializable]
        private class JsonParameter
        {
            public string name;
            public JsonParameterValue value;
        }

        [System.Serializable]
        private class JsonParameterValue
        {
            public string type;
            public string description;
        }

        private static string JsonTypeFor(Type clrType)
        {
            if (clrType.IsArray)
                return "array";

            switch (Type.GetTypeCode(clrType))
            {
                case TypeCode.String:
                    return "string";
                case TypeCode.Boolean:
                    return "boolean";
                case TypeCode.SByte:
                case TypeCode.Byte:
                case TypeCode.Int16:
                case TypeCode.UInt16:
                case TypeCode.Int32:
                case TypeCode.UInt32:
                case TypeCode.Int64:
                case TypeCode.UInt64:
                    return "integer";
                case TypeCode.Single:
                case TypeCode.Double:
                case TypeCode.Decimal:
                    return "number";
                default:
                    return "object";
            }
        }

        private static ToolCallback CreateTrampoline(
            Delegate userDelegate,
            ParameterInfo[] parameterMeta
        )
        {
            return (IntPtr jsonPtr) =>
            {
                string inboundJson = Marshal.PtrToStringAnsi(jsonPtr);

                var rustArgs = JsonUtility.FromJson<Dictionary<string, object>>(inboundJson);

                object[] args = parameterMeta
                    .Select(paramInfo =>
                    {
                        if (rustArgs.TryGetValue(paramInfo.Name, out object value))
                        {
                            return value;
                        }

                        return paramInfo.ParameterType.IsValueType
                            ? Activator.CreateInstance(paramInfo.ParameterType)
                            : null;
                    })
                    .ToArray();

                try
                {
                    object result = userDelegate.DynamicInvoke(args);
                    string resultString = result?.ToString() ?? string.Empty;
                    return Marshal.StringToHGlobalAnsi(resultString);
                }
                catch (Exception ex)
                {
                    // Log the error and return an error message
                    Debug.LogError($"Tool invocation failed: {ex.Message}");
                    return Marshal.StringToHGlobalAnsi($"Error: {ex.Message}");
                }
            };
        }

        private string BuildSchemaForDelegate(Delegate delegateInstance)
        {
            var properties = new List<JsonParameter>();
            var required = new List<string>();

            foreach (ParameterInfo parameter in delegateInstance.Method.GetParameters())
            {
                var jsonType = JsonTypeFor(parameter.ParameterType);
                if (jsonType == "object")
                {
                    throw new Exception(
                        $"Parameter {parameter.Name} is of type {parameter.ParameterType.Name}, which is not supported. Please use a supported type."
                    );
                }
                properties.Add(
                    new JsonParameter
                    {
                        name = parameter.Name,
                        value = new JsonParameterValue { type = jsonType },
                    }
                );
                required.Add(parameter.Name);
            }

            return JsonUtility.ToJson(
                new JsonSchema
                {
                    type = "object",
                    properties = new JsonProperties { properties = properties },
                    required = required,
                }
            );
        }
    }
}
