using UnityEngine;
using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;
using UnityEditor;

namespace NobodyWho
{
    public class Model : MonoBehaviour
    {
        public string modelPath = "model.gguf";

        public bool useGpuIfAvailable = true;
        private IntPtr modelHandle;

        public IntPtr GetModel()
        {
            try
            {
                string fullPath = Path.Combine(Application.streamingAssetsPath, modelPath);
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                var result = NativeBindings.get_model(
                    modelHandle,
                    fullPath,
                    useGpuIfAvailable,
                    errorBuffer
                );

                if (result == IntPtr.Zero)
                {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }

                modelHandle = result;
                return modelHandle;
            }
            catch (Exception e)
            {
                throw new NobodyWhoException(e.Message);
            }
        }

        void OnDestroy()
        {
            // we cant destroy something that does not exist
            if (modelHandle != IntPtr.Zero)
            {
                NativeBindings.destroy_model(modelHandle);
            }
        }
    }
}
