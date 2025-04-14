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
            Debug.Log(
                "[DEBUG] Starting embedding test, embedding model: " + embedding.model.modelPath
            );
            float[] dragonHillEmbedding = await embedding.Embed("The dragon is on the hill.");
            Assert.IsNotNull(dragonHillEmbedding, "First embedding not received");
            Debug.Log("[DEBUG] First embedding received: " + dragonHillEmbedding);
            float[] dragonHungryEmbedding = await embedding.Embed(
                "The dragon is hungry for humans."
            );
            Assert.IsNotNull(dragonHungryEmbedding, "Second embedding not received");
            Debug.Log("[DEBUG] Second embedding received: " + dragonHungryEmbedding);
            float[] unrelatedEmbedding = await embedding.Embed("This does not matter.");
            Assert.IsNotNull(unrelatedEmbedding, "Third embedding not received");

            Debug.Log(
                "Embedding complete: "
                    + dragonHillEmbedding
                    + " "
                    + dragonHungryEmbedding
                    + " "
                    + unrelatedEmbedding
            );

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
    }
}
