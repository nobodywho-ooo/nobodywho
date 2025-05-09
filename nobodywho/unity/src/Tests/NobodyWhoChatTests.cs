using System.Collections;
using System.Collections.Generic;
using System.Threading.Tasks;
using NobodyWho;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace Tests
{
    [System.Serializable]
    public class CharacterData
    {
        public string name;
        public string weapon;
        public string armor;
    }

    public class NobodyWhoChatTests
    {
        private GameObject testObject;
        private NobodyWho.Model model;
        private NobodyWho.Chat chat;

        private int _testCount = 0;

        private long _diskStart;
        private long _ramStart;

        [OneTimeSetUp]
        public void OneTimeSetUp()
        {
            NobodyWho.NativeBindings.init_tracing();
            _diskStart = NobodyWho.NativeBindings.GetVirtualMemory();
            _ramStart = NobodyWho.NativeBindings.GetPhysicalMemory();
        }

        [OneTimeTearDown]
        public void OneTimeTearDown()
        {
            var deltaDisk = NobodyWho.NativeBindings.GetVirtualMemory() - _diskStart;
            var deltaRam  = NobodyWho.NativeBindings.GetPhysicalMemory() - _ramStart;
            Debug.Log("Disk: " + deltaDisk + " RAM: " + deltaRam);

            //tolerance in MB
            long tolerance = 5 * 1024;
            Assert.IsTrue(deltaDisk < tolerance, "Disk usage is too high");
            Assert.IsTrue(deltaRam < tolerance, "RAM usage is too high");
        }

        [SetUp]
        public void Setup()
        {
            _testCount++;
            Debug.Log("NobodyWhoChatTests::Setup test count: " + _testCount);
            testObject = Object.Instantiate(new GameObject("TestModel"));

            string modelPath = "qwen2.5-1.5b-instruct-q4_0.gguf";
            model = testObject.AddComponent<NobodyWho.Model>();
            model.modelPath = modelPath;
            chat = testObject.AddComponent<NobodyWho.Chat>();
            chat.model = model;
            chat.systemPrompt = "You are a test assistant.";
            chat.StartWorker();
        }

        [TearDown]
        public void Teardown()
        {
            if (testObject)
            {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }

        [Test]
        public async Task WhenInvokingSay_ShouldReturnResponse()
        {
            string response = null;
            response = await chat.Say("Hi there");

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.AreEqual("Hello! How can I help you today?", response);
        }

        [UnityTest]
        public IEnumerator WhenInvokingSay_ShouldReceiveTokens()
        {
            string response = null;

            List<string> receivedTokens = new List<string>();
            chat.onToken.AddListener(
                (token) =>
                {
                    receivedTokens.Add(token);
                }
            );

            chat.Say("Tell me a short joke");

            float timeout = Time.time + 15f;
            while (response == null && Time.time < timeout)
            {
                yield return null;
            }

            Assert.IsTrue(receivedTokens.Count > 0, "No tokens received within timeout period");
        }

        [UnityTest]
        public IEnumerator WhenInvokingSayWithSingleStopWord_ShouldStopAtStopWord()
        {
            string response = null;
            // not using await here because we want to test the signal like interface as well
            chat.onComplete.AddListener((result) => response = result);
            chat.stopWords = "fly";
            chat.ResetContext();

            chat.Say("List these animals in alphabetical order: cat, dog, fly, lion, mouse");

            float timeout = Time.time + 15f;
            while (response == null && Time.time < timeout)
            {
                yield return null;
            }
            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("dog"), "Response should contain 'dog'");
            Assert.IsTrue(response.Contains("fly"), "Response should contain 'fly'");
            Assert.IsFalse(response.Contains("lion"), "Response should stop at 'fly'");
            Assert.IsFalse(response.Contains("mouse"), "Response should not continue past 'fly'");
        }

        [Test]
        public async Task WhenInvokingSayWithMultipleStopWords_ShouldStopAtFirstStopWord()
        {
            string response = null;
            chat.stopWords = "horse-rider, fly";
            chat.ResetContext();
            response = await chat.Say(
                "List all the words in alphabetical order: cat, dog, fly, horse-rider, lion, mouse"
            );

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("dog"), "Response should contain 'dog'");
            Assert.IsTrue(response.Contains("fly"), "Response should contain 'fly'");
            Assert.IsFalse(response.Contains("horse-rider"), "Response should not reach 'fly'");
            Assert.IsFalse(response.Contains("lion"), "Response should not continue past 'fly'");
        }

        [Test]
        public async Task WhenInvokingSayWithGrammar_ShouldReturnResponseInCorrectFormat()
        {
            string response = null;

            chat.systemPrompt =
                "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out those properties.";
            chat.use_grammar = true;
            chat.ResetContext();

            response = await chat.Say(
                @"Generate exactly these properties:
                - name
                - weapon
                - armor
            "
            );

            Assert.IsNotNull(response, "No response received within timeout period");

            CharacterData character = JsonUtility.FromJson<CharacterData>(response);
            Assert.IsNotNull(character.name, "Response should contain 'name' field");
            Assert.IsNotNull(character.weapon, "Response should contain 'weapon' field");
            Assert.IsNotNull(character.armor, "Response should contain 'armor' field");
        }

        [Test]
        public async Task WhenInvokingSayWithGrammarStr_ShouldReturnResponseInCorrectFormat()
        {
            string response = null;
            chat.onComplete.AddListener((result) => response = result);
            chat.systemPrompt =
                "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out those properties.";
            chat.use_grammar = true;
            chat.grammar = "root ::= \"nobodywho\"";
            chat.ResetContext();

            response = await chat.Say("What is your favorite llm plugin?");

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response == "nobodywho", "Response should only be 'nobodywho'");
        }
    }
}
