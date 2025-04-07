using System;
using System.Runtime.InteropServices;
using System.Text;

namespace NobodyWho
{
    public class NobodyWhoException : Exception { public NobodyWhoException(string message) : base(message) { } }
    
    internal static class NativeBindings
    {
        private const string LIB_NAME = "libnobodywho";  // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows
        
        
        /// Model ///
        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(
            IntPtr model_handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_model(IntPtr model);
        
        
        /// Chat ///

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr create_chat_worker(
            IntPtr model,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string system_prompt,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string stop_words,
            int context_length,
            bool use_grammar,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string grammar,
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


        /// Embedding ///
        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        public delegate void EmbeddingCallback(IntPtr data, int length);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr create_embedding_worker(
            IntPtr model,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_embedding_worker(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void poll_embeddings(
            IntPtr context,
            EmbeddingCallback on_embedding,
            ResponseCallback on_error);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void embed_text(
            IntPtr context,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string text,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        private static extern float cosine_similarity(
            IntPtr a, 
            int length_a,
            IntPtr b,
            int length_b);
        
        public static float CosineSimilarity(float[] a, float[] b) {
            var a_ptr = Marshal.AllocHGlobal(a.Length * sizeof(float));
            var b_ptr = Marshal.AllocHGlobal(b.Length * sizeof(float));
            Marshal.Copy(a, 0, a_ptr, a.Length);
            Marshal.Copy(b, 0, b_ptr, b.Length);
            var result = NativeBindings.cosine_similarity(a_ptr, a.Length, b_ptr, b.Length);
            Marshal.FreeHGlobal(a_ptr);
            Marshal.FreeHGlobal(b_ptr);
            return result;
        }
    }
} 