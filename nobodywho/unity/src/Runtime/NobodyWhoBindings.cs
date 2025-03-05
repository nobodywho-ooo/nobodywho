using System;
using System.Runtime.InteropServices;

namespace NobodyWho
{
    public enum ModelErrorType
    {
        ModelNotFound = 1,
        InvalidModel = 2
    }

    [StructLayout(LayoutKind.Sequential)]
    internal struct ModelResult
    {
        public IntPtr handle;
        [MarshalAs(UnmanagedType.I1)]
        public bool success;
        public ModelErrorType errorType;
        public IntPtr errorMessage;
    }

    /// <summary>
    /// Native bindings to the Rust library functions
    /// </summary>
    internal static class Native
    {
        private const string LIB_NAME = "libnobodywho";  // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern ModelResult get_model(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu);
    }
} 