using System;
using System.Runtime.InteropServices;

namespace NobodyWho
{
    public class NobodyWhoException : Exception { public NobodyWhoException(string message) : base(message) { } }
    
    internal static class NativeBindings
    {
        private const string LIB_NAME = "libnobodywho";  // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu);
    }
} 