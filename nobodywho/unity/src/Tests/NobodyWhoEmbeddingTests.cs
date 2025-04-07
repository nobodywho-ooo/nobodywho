using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;
using System.Collections;
using NobodyWho;
using System.Threading.Tasks;

namespace Tests
{
    public class NobodyWhoEmbeddingTests
    {
        private GameObject testObject;
        private NobodyWho.Model model;
        private NobodyWho.Embedding embedding;

        [SetUp]
        public void Setup()
        {
            testObject = new GameObject("TestModel");
            model = testObject.AddComponent<NobodyWho.Model>();
            model.modelPath = "bge-small-en-v1.5-q8_0.gguf";
            
            embedding = testObject.AddComponent<NobodyWho.Embedding>();
            embedding.model = model;
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
        public async Task WhenComparingEmbeddings_SimilarTextsShouldHaveHigherSimilarity()
        {
            float[] dragonHillEmbedding = await embedding.Embed("The dragon is on the hill.");
            float[] dragonHungryEmbedding = await embedding.Embed("The dragon is hungry for humans.");
            float[] unrelatedEmbedding = await embedding.Embed("This does not matter.");

            Debug.Log("Embedding complete: " + dragonHillEmbedding + " " + dragonHungryEmbedding + " " + unrelatedEmbedding);

            Assert.IsNotNull(dragonHillEmbedding, "First embedding not received");
            Assert.IsNotNull(dragonHungryEmbedding, "Second embedding not received");
            Assert.IsNotNull(unrelatedEmbedding, "Third embedding not received");

            float lowSimilarity = embedding.CosineSimilarity(unrelatedEmbedding, dragonHillEmbedding);
            float highSimilarity = embedding.CosineSimilarity(dragonHillEmbedding, dragonHungryEmbedding);

            Assert.Greater(highSimilarity, lowSimilarity, 
                $"Similar texts should have higher similarity. Low: {lowSimilarity}, High: {highSimilarity}");
        }
    }
} 