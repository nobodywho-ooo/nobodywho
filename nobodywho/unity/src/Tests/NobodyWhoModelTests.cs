using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using System.Collections;
using System;
using System.IO;
using NobodyWho;

namespace Tests
{
    public class NobodyWhoModelTests
    {
        private GameObject testObject;
        private NobodyWho.Model model;

        [SetUp]
        public void Setup()
        {
            testObject = new GameObject("TestModel");
            model = testObject.AddComponent<NobodyWho.Model>();
            Directory.CreateDirectory(Application.streamingAssetsPath);
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
            
            model.modelPath = "invalid.gguf";
            var exception = Assert.Throws<NobodyWhoException>(() => model.GetModel());
            File.Delete(tempPath);
        }
        
        // TODO: add a build step for the model in create temp project. otherwise this will fail.
        [Test] 
        public void WhenModelPathIsGGUF_ShouldLoadModel()
        {
            model.modelPath = "test_model.gguf";
            var model_handle = model.GetModel();
            Assert.That(model_handle, Is.Not.EqualTo(IntPtr.Zero));
        }
    }
} 