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
        public ModelWrapper wrapper = ModelWrapper.New("model.gguf", true);

        // only used for GUI
        public string _modelPath = "model.gguf";
        // only used for GUI
        public bool _useGpuIfAvailable = true;

        // to allow the property pattern (properties can't be serialized and unity uses serieliaed field for the inspector GUI) dwe hook into the validate and sets the values there. 
        private void OnValidate()
        {
            if (wrapper != null)
            {
                wrapper.SetModelPath(_modelPath);
                wrapper.SetUseGpuIfAvailable(_useGpuIfAvailable);
            }
        }
        public string modelPath
        {
            get { return Marshal.PtrToStringAnsi(wrapper.GetModelPath()); }
            set { wrapper.SetModelPath(value); }
        }

        public bool useGpuIfAvailable
        {
            get { return wrapper.GetUseGpuIfAvailable(); }
            set { wrapper.SetUseGpuIfAvailable(value); }
        }

        public IntPtr modelWrapperContext
        {
            get { return wrapper.Context; }
        }

        private void OnDestroy()
        {
            if (wrapper != null)
            {
                wrapper.Dispose();
                wrapper = null;
            }
        }
    }
}
