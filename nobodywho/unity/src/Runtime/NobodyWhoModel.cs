using UnityEngine;
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

namespace NobodyWho {

    public class Model : MonoBehaviour {
        public string modelPath = "model.gguf";
        public bool useGpuIfAvailable = true;
        private IntPtr modelHandle;

        public IntPtr GetModel() {
            if (modelHandle != IntPtr.Zero) {
                return modelHandle;
            }
            try {
                string fullPath = Path.Combine(Application.streamingAssetsPath, modelPath);
                var errorBuffer = new StringBuilder(256);
                var result = NativeBindings.get_model(fullPath, useGpuIfAvailable, errorBuffer);
                
                if (result == IntPtr.Zero) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
                
                modelHandle = result;
                return modelHandle;
                
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }
    }
} 