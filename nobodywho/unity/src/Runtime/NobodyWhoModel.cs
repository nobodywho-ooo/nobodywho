using UnityEngine;
using System;
using System.IO;
using System.Runtime.InteropServices;

namespace NobodyWho {

    public class Model : MonoBehaviour {
        public string modelPath = "model.gguf";
        public bool useGpuIfAvailable = true;
        private IntPtr modelHandle;

        public IntPtr GetModel() {
            if (modelHandle != IntPtr.Zero) {
                return modelHandle;
            }

            string fullPath = Path.Combine(Application.streamingAssetsPath, modelPath);

            try {
                var result = NativeBindings.get_model(fullPath, useGpuIfAvailable);
                
                // checks if the result is a string error message, or a valid model handle
                string potentialError = Marshal.PtrToStringUTF8(result); // if this
                if (!string.IsNullOrEmpty(potentialError)) {
                    throw new NobodyWhoException(potentialError);
                }

                modelHandle = result;
                return modelHandle;

            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }
    }
} 