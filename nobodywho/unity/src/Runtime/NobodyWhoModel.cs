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

        public string ModelPath
        {
            get { return Marshal.PtrToStringAnsi(wrapper.GetModelPath()); }
            set { wrapper.SetModelPath(value); }
        }

        public bool UseGpuIfAvailable
        {
            get { return wrapper.GetUseGpuIfAvailable(); }
            set { wrapper.SetUseGpuIfAvailable(value); }
        }

        public IntPtr ModelWrapperContext
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
