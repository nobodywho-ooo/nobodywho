using System;
using System.Runtime.InteropServices;

namespace NobodyWho
{
    /// <summary>
    /// Native bindings to the NobodyWho core library.
    /// This class provides direct access to the Rust-based functionality.
    /// </summary>
    public static class NobodyWhoNative
    {
        // The name of the native library - should match the output DLL name from Rust
        private const string NativeLibrary = "nobodywho";

        /// <summary>
        /// Loads a model from the specified path.
        /// </summary>
        /// <param name="path">Full path to the model file (.gguf)</param>
        /// <param name="useGpu">Whether to use GPU acceleration if available</param>
        /// <returns>Pointer to the loaded model, or IntPtr.Zero if loading failed</returns>
        [DllImport(NativeLibrary, CallingConvention = CallingConvention.Cdecl)]
        public static extern IntPtr get_model(string path, bool useGpu);
    }
}



