using System.Collections;
using System.Threading.Tasks;
using NobodyWho;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

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
            NobodyWhoBindings.init_tracing();
            testObject = new GameObject("TestModel");
            model = testObject.AddComponent<NobodyWho.Model>();
            model.ModelPath = System.IO.Path.Combine(Application.streamingAssetsPath, "bge-small-en-v1.5-q8_0.gguf");

            embedding = testObject.AddComponent<NobodyWho.Embedding>();
            embedding.model = model;
            embedding.StartWorker();
        }

        [Test]
        public void WhenComparingEmbeddings_SimilarTextsShouldHaveHigherSimilarity()
        {
            embedding.Embed("The dragon is on the hill.");
            float[] dragonHillEmbedding = embedding.GetEmbeddingBlocking();
            Assert.IsNotNull(dragonHillEmbedding, "First embedding not received");

            embedding.Embed("The dragon is hungry for humans.");
            float[] dragonHungryEmbedding = embedding.GetEmbeddingBlocking();
            Assert.IsNotNull(dragonHungryEmbedding, "Second embedding not received");

            embedding.Embed("This does not matter.");
            float[] unrelatedEmbedding = embedding.GetEmbeddingBlocking();
            Assert.IsNotNull(unrelatedEmbedding, "Third embedding not received");

            Assert.IsNotNull(dragonHillEmbedding, "First embedding not received");
            Assert.IsNotNull(dragonHungryEmbedding, "Second embedding not received");
            Assert.IsNotNull(unrelatedEmbedding, "Third embedding not received");

            float lowSimilarity = embedding.CosineSimilarity(
                unrelatedEmbedding,
                dragonHillEmbedding
            );
            float highSimilarity = embedding.CosineSimilarity(
                dragonHillEmbedding,
                dragonHungryEmbedding
            );

            Assert.Greater(
                highSimilarity,
                lowSimilarity,
                $"Similar texts should have higher similarity. Low: {lowSimilarity}, High: {highSimilarity}"
            );
        }

        [TearDown]
        public void Teardown()
        {
            // Remove any event listeners added during tests
            embedding.onEmbeddingComplete.RemoveAllListeners();

            // Destroy the test object and its components
            if (testObject)
            {
                UnityEngine.Object.DestroyImmediate(testObject);
            }
        }
    }
}
