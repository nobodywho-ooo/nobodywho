using System;
using System.Runtime.InteropServices;
using System.Text;

namespace NobodyWho
{
    public class NobodyWhoException : Exception { public NobodyWhoException(string message) : base(message) { } }
    
    internal static class NativeBindings
    {
        private const string LIB_NAME = "libnobodywho";  // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr create_chat_worker(
            IntPtr model,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string system_prompt,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string stop_tokens,
            int context_length,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);  

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        public delegate void ResponseCallback([MarshalAs(UnmanagedType.LPUTF8Str)] string text);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void poll_responses(
            IntPtr context,
            ResponseCallback on_token,
            ResponseCallback on_complete,
            ResponseCallback on_error);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void send_prompt(
            IntPtr context,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string prompt,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_chat_worker(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_model(IntPtr model);
    }
} 