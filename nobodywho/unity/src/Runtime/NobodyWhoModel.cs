using UnityEngine;

namespace NobodyWho
{
    public class Model : MonoBehaviour
    {
        public string modelPath = "model.gguf";
        public bool useGpuIfAvailable = true;
        private Model model;

        // Using unsafe to allow direct memory access since Rust data structures 
        // are passed as raw pointers that need to be marshalled in C#
        public unsafe Model GetModel()
        {
            if (model != null)
            {
                return model;
            }

            string fullPath = System.IO.Path.Combine(Application.streamingAssetsPath, modelPath);

            try
            {
                // The Rust code uses llm::get_model, which gets exported as get_model in the DLL
                // We need to declare this import somewhere (likely in a NativeBindings.cs file):
                // [DllImport("nobodywho")]
                // public static extern IntPtr get_model(string path, bool use_gpu);
                IntPtr modelHandle = NobodyWhoBindings.get_model(fullPath, useGpuIfAvailable);
                if (modelHandle == IntPtr.Zero)
                {
                    throw new System.Exception("Failed to load model - null pointer returned from get_model");
                }
                
                model = new Model(modelHandle); // Wrap the native pointer in managed Model class
                return model;
            }
            catch (System.Exception e)
            {
                Debug.LogError($"Could not load model: {e.Message}");
                throw;
            }
        }

        
    }
} 