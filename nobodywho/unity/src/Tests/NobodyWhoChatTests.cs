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
        private float _timeoutDuration = 60 * 5; // 5 minutes

        [OneTimeSetUp]
        public void OneTimeSetUp()
        {
            NobodyWho.NobodyWhoBindings.init_tracing();
        }

        [SetUp]
        public void Setup()
        {
            _testCount++;
            Debug.Log("NobodyWhoChatTests::Setup test count: " + _testCount);
            testObject = Object.Instantiate(new GameObject("TestModel"));

            model = testObject.AddComponent<NobodyWho.Model>();
            model.modelPath = System.IO.Path.Combine(Application.streamingAssetsPath, "qwen2.5-1.5b-instruct-q4_0.gguf");

            chat = testObject.AddComponent<NobodyWho.Chat>();
            chat.model = model;
            chat.systemPrompt = "You are a test assistant.";
            chat.StartWorker();
        }

        [TearDown]
        public void Teardown()
        {
            // Remove any event listeners added during tests
            chat.responseFinished.RemoveAllListeners();
            chat.responseUpdated.RemoveAllListeners();

            // Destroy the test object and its components
            if (testObject)
            {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }

        [Test]
        public void WhenInvokingSay_ShouldReturnResponse()
        {
            chat.Say("What is the capital of Denmark?");
            string response = chat.GetResponseBlocking();
            System.Console.WriteLine(response);

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("Copenhagen"), "Got wrong response: " + response);
        }

        [UnityTest]
        public IEnumerator WhenInvokingSay_ShouldReceiveTokens()
        {
            string response = null;
            List<string> receivedTokens = new List<string>();
            chat.responseFinished.AddListener((result) => response = result);
            chat.responseUpdated.AddListener(
                (token) =>
                {
                    receivedTokens.Add(token);
                }
            );

            chat.Say("Tell me a short joke");

            float timeout = Time.time + _timeoutDuration;
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
            chat.responseFinished.AddListener((result) => response = result);
            chat.stopWords = "fly";
            chat.ResetContext();

            chat.Say("List these animals in alphabetical order: cat, dog, fly, lion, mouse");

            float timeout = Time.time + _timeoutDuration;
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
            chat.stopWords = "horse-rider, fly";
            chat.ResetContext();

            chat.Say(
                "List all the words in alphabetical order: cat, dog, fly, horse-rider, lion, mouse"
            );

            var response = chat.GetResponseBlocking();

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response.Contains("dog"), "Response should contain 'dog'");
            Assert.IsTrue(response.Contains("fly"), "Response should contain 'fly'");
            Assert.IsFalse(response.Contains("horse-rider"), "Response should not reach 'fly'");
            Assert.IsFalse(response.Contains("lion"), "Response should not continue past 'fly'");
        }

        [Test]
        public void WhenInvokingSayWithGrammar_ShouldReturnResponseInCorrectFormat()
        {

            chat.systemPrompt =
                "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out those properties.";
            chat.useGrammar = true;
            chat.ResetContext();

            chat.Say(
                @"Generate exactly these properties:
                - name
                - weapon
                - armor
            "
            );

            string response = chat.GetResponseBlocking();

            CharacterData character = JsonUtility.FromJson<CharacterData>(response);
            Assert.IsNotNull(character.name, "Response should contain 'name' field");
            Assert.IsNotNull(character.weapon, "Response should contain 'weapon' field");
            Assert.IsNotNull(character.armor, "Response should contain 'armor' field");
        }

        [Test]
        public void WhenInvokingSayWithGrammarStr_ShouldReturnResponseInCorrectFormat()
        {
            chat.systemPrompt =
                "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out those properties.";
            chat.useGrammar = true;
            chat.grammar = "root ::= \"nobodywho\"";
            chat.ResetContext();

            chat.Say("What is your favorite llm plugin?");
            string response = chat.GetResponseBlocking();

            Assert.IsNotNull(response, "No response received within timeout period");
            Assert.IsTrue(response == "nobodywho", "Response should only be 'nobodywho'");
        }

        [UnityTest]
        public IEnumerator WhenInvokingSayAndStopping_ShouldReturnIncompleteResponse()
        {
            chat.systemPrompt = "You are a counter, only outputting numbers";
            chat.ResetContext();

            string response = null;
            chat.responseFinished.AddListener((result) => response = result);
            List<string> receivedTokens = new List<string>();
            chat.responseUpdated.AddListener(
                (token) =>
                {
                    if (token.Contains("5"))
                    {
                        chat.Stop();
                    }
                }
            );

            chat.Say("Count from 0 to 9");

            float timeout = Time.time + _timeoutDuration;
            while (response == null && Time.time < timeout)
            {
                yield return null;
            }

            Assert.IsTrue(response.Contains("5"), "Response should contain '5'");
            Assert.IsFalse(response.Contains("9"), "Response should not contain '9'");
        }
    }
}
