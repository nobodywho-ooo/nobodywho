using System;
using System.Runtime.InteropServices;

namespace NobodyWho
{
    /// <summary>
    /// Native bindings to the Rust library functions
    /// </summary>
    internal static class Native
    {
        private const string LIB_NAME = "libnobodywho";  // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu);

    }

} 