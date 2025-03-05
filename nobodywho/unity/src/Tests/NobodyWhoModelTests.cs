using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using System.Collections;
using System;

namespace NobodyWho.Tests
{
    public class NobodyWhoModelTests
    {
        private GameObject testObject;
        private NobodyWhoModel model;

        [SetUp]
        public void Setup()
        {
            testObject = new GameObject("TestModel");
            model = testObject.AddComponent<NobodyWhoModel>();
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
        public void WhenModelPathIsInvalid_ShouldReturnError()
        {
            
            model.ModelPath = "/path/that/does/not/exist.gguf";
            var exception = Assert.Throws<FileNotFoundException>(() => model.LoadModel());
            Assert.That(exception.Message, Contains.Substring("Model file not found"));
        }
        
        // TODO: add a build step for the model in create temp project. otherwise this will fail.
        [Test] 
        public void WhenModelPathIsGGUF_ShouldLoadModel()
        {
            model.ModelPath = "test_model.gguf";
            var result = model.LoadModel();
            Assert.That(result, Is.Not.Null);
        }
        
    }
} 