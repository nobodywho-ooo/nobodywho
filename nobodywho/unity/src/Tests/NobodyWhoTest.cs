using System;
using System.Runtime.InteropServices;
using UnityEngine;
using UnityEngine.TestTools;
using NUnit.Framework;
using System.Collections;

namespace NobodyWho.Tests
{
    public class NobodyWhoTest
    {
        // Direct DLL imports - using existing functions from the core library
        [DllImport("nobodywho")]
        private static extern IntPtr llm_get_model(string modelPath, bool useGpu);
        
        [DllImport("nobodywho")]
        private static extern void llm_destroy_model(IntPtr model);
        
        [Test]
        public void ExampleTest()
        {
            // Example test
            Assert.Pass();
        }
        
        [UnityTest]
        public IEnumerator TestCoreLibraryLoading()
        {
            // This test simply verifies that we can load the DLL and call a function
            try
            {
                // Just verify the function can be called without crashing
                // We don't care about the result, just that it doesn't throw
                llm_get_model("dummy_path.gguf", false);
                
                Debug.Log("Successfully called core library function");
                Assert.Pass("Core library loaded and function called successfully");
            }
            catch (DllNotFoundException ex)
            {
                Assert.Fail($"Failed to load core library: {ex.Message}");
            }
            catch (EntryPointNotFoundException ex)
            {
                Assert.Fail($"Failed to find function in core library: {ex.Message}");
            }
            
            yield return null;
        }
    }
} 