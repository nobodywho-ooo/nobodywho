using UnityEngine;
using System;



namespace NobodyWho
{
    public class Model : MonoBehaviour
    {
        // TODO: make a file picker for this instead. for now, just use the StreamingAssets folder.
        public string modelPath = "model.gguf";
        public bool useGpuIfAvailable = true;
        private IntPtr modelHandle;


        // Return a pointer to the model instead of a managed object
        // TODO: implement marshalling, maybe? 
        public IntPtr GetModel()
        {
            if (modelHandle != IntPtr.Zero)
            {
                return modelHandle;
            }

            string fullPath = System.IO.Path.Combine(Application.streamingAssetsPath, modelPath);

            try
            {
                modelHandle = Native.get_model(fullPath, useGpuIfAvailable);
                if (modelHandle == IntPtr.Zero)
                {
                    throw new System.Exception("Failed to load model - null pointer returned from get_model");
                }
                return modelHandle;
            }
            catch (System.Exception e)
            {
                Debug.LogError($"Could not load model: {e.Message}");
                throw;
            }
        }

        
    }
} 