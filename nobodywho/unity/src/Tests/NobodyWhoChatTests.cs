using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using System.Collections;
using NobodyWho;


namespace Tests
{
    public class NobodyWhoChatTests
    {
        private GameObject testObject;
        private NobodyWho.Model model;
        private NobodyWho.Chat chat;

        [SetUp]
        public void Setup() {
            testObject = new GameObject("TestModel");
            model = testObject.AddComponent<NobodyWho.Model>();
            model.modelPath = "qwen2.5-1.5b-instruct-q4_0.gguf";
            chat = testObject.AddComponent<NobodyWho.Chat>();
            chat.model = model;
        }

        [TearDown]
        public void Teardown() {
            if (testObject != null) {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }

        [Test]
        public void WhenInvokingSay_ShouldReturnResponse() {
            string response = null;
            chat.systemPrompt = "You are a test assistant.";
            chat.onComplete.AddListener((result) => response = result);
            
            chat.say("Hi there");
            
            // Wait for response with timeout
            var timeout = Time.time + 5f; // 5 second timeout
            while (response == null && Time.time < timeout) {
                // Let Unity process events
            }
            
            Assert.AreEqual("Hello, how can I help you?", response);
        }
    }
}