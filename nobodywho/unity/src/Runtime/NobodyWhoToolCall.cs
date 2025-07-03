using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using System.Runtime.InteropServices;
using UnityEngine;

namespace NobodyWho
{
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

        [System.Serializable]
        private class JsonArguments
        {
            // Unity will populate fields that match the JSON property names
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

        private static object ConvertJsonValueToType(object value, Type targetType)
        {
            if (value == null)
                return null;

            string stringValue = value.ToString();

            if (targetType == typeof(string))
                return stringValue;
            if (targetType == typeof(bool))
                return bool.Parse(stringValue);
            if (targetType == typeof(int))
                return int.Parse(stringValue);
            if (targetType == typeof(float))
                return float.Parse(stringValue);
            if (targetType == typeof(double))
                return double.Parse(stringValue);
            if (targetType == typeof(long))
                return long.Parse(stringValue);

            try
            {
                return JsonUtility.FromJson(stringValue, targetType);
            }
            catch
            {
                return targetType.IsValueType ? Activator.CreateInstance(targetType) : null;
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

                // Use reflection to get parameter values from JSON
                var jsonObj = JsonUtility.FromJson<JsonArguments>(inboundJson);

                object[] args = parameterMeta.Select(paramInfo =>
                    {
                        if (jsonObj != null)
                        {
                            var field = jsonObj.GetType().GetField(paramInfo.Name);
                            if (field != null)
                            {
                                return field.GetValue(jsonObj);
                            }
                        }
                        return paramInfo.ParameterType.IsValueType
                            ? Activator.CreateInstance(paramInfo.ParameterType)
                            : null;
                    })
                    .ToArray();

                object result = userDelegate.DynamicInvoke(args);

                return Marshal.StringToHGlobalAnsi(result.ToString());
            };
        }

        private string BuildSchemaForDelegate(Delegate delegateInstance)
        {
            var properties = new List<JsonParameter>();
            var required = new List<string>();

            foreach (ParameterInfo parameter in delegateInstance.Method.GetParameters())
            {
                properties.Add(
                    new JsonParameter
                    {
                        name = parameter.Name,
                        value = new JsonParameterValue
                        {
                            type = JsonTypeFor(parameter.ParameterType),
                        },
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
