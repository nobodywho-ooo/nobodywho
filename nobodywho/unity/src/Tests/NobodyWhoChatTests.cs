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
            
            float timeout = Time.time + 15f; // 15 second timeout

            // let unity process stuff until we get a response
            while (response == null && Time.time < timeout) {
                yield return null;
            }
            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.AreEqual("Hello! How can I help you today?", response);
        }

        [UnityTest]
        public IEnumerator WhenInvokingSay_ShouldReceiveTokens() {
            string response = null;
            chat.onComplete.AddListener((result) => response = result);            
            // Setup token collection
            List<string> receivedTokens = new List<string>();
            chat.onToken.AddListener((token) => {
                receivedTokens.Add(token);
            });
            
            chat.say("Tell me a short joke");
            
            float timeout = Time.time + 15f;
            while (response == null && Time.time < timeout) {
                yield return null;
            }

            Assert.IsTrue(receivedTokens.Count > 0, "No tokens received within timeout period");            
        }


        [UnityTest]
        public IEnumerator WhenInvokingSayWithSingleStopToken_ShouldStopAtStopToken() {
            string response = null;
            chat.onComplete.AddListener((result) => response = result);
            chat.stopTokens = new string[] { "fly" }; // Set the stop token
            chat.say("List these animals in alphabetical order: cat, dog, fly, lion, mouse");

            float timeout = Time.time + 15f;
            while (response == null && Time.time < timeout) {
                yield return null;
            }
            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("dog"), "Response should contain 'dog'");
            Assert.IsTrue(response.Contains("fly"), "Response should contain 'fly'");
            Assert.IsFalse(response.Contains("lion"), "Response should stop at 'fly'");
            Assert.IsFalse(response.Contains("mouse"), "Response should not continue past 'fly'");
        }

        [UnityTest]
        public IEnumerator WhenInvokingSayWithMultipleStopTokens_ShouldStopAtFirstStopToken() {
            string response = null;
            chat.onComplete.AddListener((result) => response = result);
            chat.stopTokens = new string[] { "horse-rider", "fly" }; // Set the stop token
            chat.say("List all the words in alphabetical order: dog, fly, horse-rider, lion, mouse");

            float timeout = Time.time + 15f;
            while (response == null && Time.time < timeout) {
                yield return null;
            }
            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("dog"), "Response should contain 'dog'");
            Assert.IsTrue(response.Contains("fly"), "Response should contain 'fly'");
            Assert.IsFalse(response.Contains("horse-rider"), "Response should not reach 'fly'");
            Assert.IsFalse(response.Contains("lion"), "Response should not continue past 'fly'");
        }
    }
}