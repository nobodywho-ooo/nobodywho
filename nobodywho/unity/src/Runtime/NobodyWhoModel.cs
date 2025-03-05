using UnityEngine;
using System;
using System.IO;
using System.Runtime.InteropServices;

namespace NobodyWho
{
    public class ModelLoadException : Exception
    {
        public ModelErrorType ErrorType { get; }

        public ModelLoadException(ModelErrorType errorType, string message) 
            : base(message)
        {
            ErrorType = errorType;
        }
    }

    public class Model : MonoBehaviour
    {
        // TODO: make a file picker for this instead. for now, just use the StreamingAssets folder.
        public string modelPath = "model.gguf";
        public bool useGpuIfAvailable = true;
        private IntPtr modelHandle;

        // Return a pointer to the model instead of a managed object
        public IntPtr GetModel()
        {
            if (modelHandle != IntPtr.Zero)
            {
                return modelHandle;
            }

            string fullPath = Path.Combine(Application.streamingAssetsPath, modelPath);

            try
            {
                var result = Native.get_model(fullPath, useGpuIfAvailable);
                
                if (!result.success)
                {
                    string errorMessage = "Unknown error";
                    if (result.errorMessage != IntPtr.Zero)
                    {
                        errorMessage = Marshal.PtrToStringUTF8(result.errorMessage);
                    }
                    
                    throw new ModelLoadException(result.errorType, errorMessage);
                }

                modelHandle = result.handle;
                return modelHandle;
            }
            catch (ModelLoadException)
            {
                throw; // Re-throw ModelLoadException as is
            }
            catch (Exception e)
            {
                Debug.LogError($"Could not load model: {e.Message}");
                throw;
            }
        }
    }
} 