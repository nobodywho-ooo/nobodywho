using System;
using System.Runtime.InteropServices;
using System.Text;

namespace NobodyWho
{
    public class NobodyWhoException : Exception
    {
        public NobodyWhoException(string message)
            : base(message) { }
    }

    public static class NativeBindings
    {
        private const string LIB_NAME = "nobodywho"; // Will be libnobodywho.so on Linux, libnobodywho.dylib on Mac, nobodywho.dll on Windows - this catches all the cases.

        /// tracing setup - only useful in tests
        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr init_tracing();

        /// Model ///
        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(
            IntPtr model_handle,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string model_path,
            bool use_gpu,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf
        );

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_model(IntPtr model);

        /// Embedding ///

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        public delegate void EmbeddingCallback(IntPtr caller, IntPtr data, int length);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr create_embedding_worker(
            IntPtr model,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf
        );

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_embedding_worker(IntPtr context);

        // a struct for the return type of embeddings operations
        [StructLayout(LayoutKind.Sequential)]
        public struct FloatArray
        {
            public IntPtr Data;
            public UIntPtr Length;

            public float[] ToManagedArray()
            {
                if (Data == IntPtr.Zero || Length.ToUInt64() == 0)
                    return Array.Empty<float>();

                int length = (int)Length.ToUInt64();
                float[] result = new float[length];
                Marshal.Copy(Data, result, 0, length);
                return result;
            }
        }

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_float_array(FloatArray array);

        public static float[] PollEmbeddings(IntPtr context)
        {
            FloatArray array = poll_embed_result(context);
            try
            {
                if (array.Data == IntPtr.Zero || array.Length.ToUInt64() == 0)
                {
                    return null;
                }

                float[] managedArray = array.ToManagedArray();
                return managedArray;
            }
            finally
            {
                // always free data on the rust side, we're done with it.
                destroy_float_array(array);
            }
        }

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern FloatArray poll_embed_result(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void embed_text(
            IntPtr context,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string text,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf
        );

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        private static extern float cosine_similarity(
            IntPtr a,
            int length_a,
            IntPtr b,
            int length_b
        );

        public static float CosineSimilarity(float[] a, float[] b)
        {
            var a_ptr = Marshal.AllocHGlobal(a.Length * sizeof(float));
            var b_ptr = Marshal.AllocHGlobal(b.Length * sizeof(float));
            Marshal.Copy(a, 0, a_ptr, a.Length);
            Marshal.Copy(b, 0, b_ptr, b.Length);
            var result = NativeBindings.cosine_similarity(a_ptr, a.Length, b_ptr, b.Length);
            Marshal.FreeHGlobal(a_ptr);
            Marshal.FreeHGlobal(b_ptr);
            return result;
        }

        /// Chat ///

        [UnmanagedFunctionPointer(CallingConvention.Cdecl)]
        public delegate void ChatTokenCallback(IntPtr caller, IntPtr token);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr create_chat_worker(
            IntPtr model,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string system_prompt,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string stop_words,
            int context_length,
            bool use_grammar,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string grammar,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf
        );

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void send_prompt(
            IntPtr context,
            [MarshalAs(UnmanagedType.LPUTF8Str)] string prompt,
            [MarshalAs(UnmanagedType.LPStr)] StringBuilder error_buf
        );

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr poll_token(IntPtr context);

        // try to marshal to string
        // consumes and always frees the pointer
        private static string ptr_to_str(IntPtr ptr)
        {
            // check for null ptr
            if (ptr == IntPtr.Zero)
            {
                return null;
            }

            string result = null;
            try
            {
                return Marshal.PtrToStringUTF8(ptr);
            }
            catch (Exception ex)
            {
                // TODO: handle error
                return null;
            }
            finally
            {
                destroy_string(ptr);
            }
        }

        public static string PollToken(IntPtr context)
        {
            IntPtr ptr = poll_token(context);
            return ptr_to_str(ptr);
        }

        public static string PollResponse(IntPtr context)
        {
            IntPtr ptr = poll_response(context);
            return ptr_to_str(ptr);
        }

        public static string PollError(IntPtr context)
        {
            IntPtr ptr = poll_error(context);
            return ptr_to_str(ptr);
        }

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr poll_response(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr poll_error(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_string(IntPtr s);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        public static extern void destroy_chat_worker(IntPtr context);

        [DllImport(LIB_NAME, CallingConvention = CallingConvention.Cdecl)]
        private static extern void get_memory_stats(long[] out_stats);

        public static long GetPhysicalMemory()
        {
            long[] statsArray = new long[2];
            get_memory_stats(statsArray);
            return statsArray[0];
        }

        public static long GetVirtualMemory()
        {
            long[] statsArray = new long[2];
            get_memory_stats(statsArray);
            return statsArray[1];
        }
    }
}
