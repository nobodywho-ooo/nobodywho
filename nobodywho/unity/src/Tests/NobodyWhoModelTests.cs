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
            NobodyWhoBindings.init_tracing();
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
    }
}
