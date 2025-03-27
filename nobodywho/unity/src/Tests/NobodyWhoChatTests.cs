using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using System.Collections;
using System.Collections.Generic;
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
            chat.systemPrompt = "You are a test assistant.";
            chat.model = model;
        }

        [TearDown]
        public void Teardown() {
            if (testObject != null) {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }

        [UnityTest]
        public IEnumerator WhenInvokingSay_ShouldReturnResponse() {
            string response = null;
            chat.onComplete.AddListener((result) => response = result);            
            chat.say("Hi there");
            
            float timeout = Time.time + 5f; // 5 second timeout

            // let unity process stuff until we get a response
            while (response == null && Time.time < timeout) {
                yield return null;
            }
            Debug.Log("Response: " + response);
            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.AreEqual("Hello! How can I help you today?", response);
            Debug.Log("DEBUG [ChatTests]: test completed");
        }

        // [UnityTest]
        // public IEnumerator WhenInvokingSay_ShouldReceiveTokens() {
        //     Debug.Log("DEBUG [ChatTests]: test started");
        //     // Setup token collection
        //     List<string> receivedTokens = new List<string>();
        //     chat.onToken.AddListener((token) => {
        //         receivedTokens.Add(token);
        //         Debug.Log($"Token received: {token}");
        //     });
            
        //     chat.say("Tell me a short joke");
            
        //     float timeout = Time.time + 5f;
        //     while (receivedTokens.Count < 5 && Time.time < timeout) {
        //         yield return null;
        //     }

        //     Assert.IsTrue(receivedTokens.Count > 0, "No tokens received within timeout period");            
        //     Debug.Log("DEBUG [ChatTests]: test completed");
        // }
    }
}