using System;
using System.Collections;
using System.IO;
using NobodyWho;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace Tests
{
    public class NobodyWhoModelTests
    {
        private GameObject testObject;

        [SetUp]
        public void Setup()
        {
            NobodyWho.NativeBindings.init_tracing();
            testObject = new GameObject("TestModel");
            testObject.AddComponent<NobodyWho.Model>();
        }

        [TearDown]
        public void Teardown()
        {
            if (testObject != null)
            {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }

        [Test]
        public void WhenModelIsWrong_ShouldThrowNobodyWhoException()
        {
            // Create a fake GGUF file with invalid content
            string tempPath = Path.Combine(Application.streamingAssetsPath, "invalid.gguf");
            File.WriteAllText(tempPath, "This is not a valid GGUF file");

            var model = testObject.GetComponent<NobodyWho.Model>();
            try
            {
                model.modelPath = tempPath;
                var exception = Assert.Throws<NobodyWhoException>(() => model.GetModel());
            }
            finally
            {
                // Ensure we clean up the temp file even if the test fails
                if (File.Exists(tempPath))
                {
                    File.Delete(tempPath);
                }
            }
        }

        [Test]
        public void WhenModelPathIsGGUF_ShouldLoadModel()
        {
            var model = testObject.GetComponent<NobodyWho.Model>();
            // TODO: add a build step in nix for the model in `create temp project`. otherwise this will fail when other people run it.
            model.modelPath = "qwen2.5-1.5b-instruct-q4_0.gguf";
            var model_handle = model.GetModel();
            Assert.That(model_handle, Is.Not.EqualTo(IntPtr.Zero));
        }
    }
}
