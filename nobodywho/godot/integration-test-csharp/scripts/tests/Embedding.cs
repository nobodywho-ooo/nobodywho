using System.Collections.Generic;
using System.Threading.Tasks;
using GdUnit4;
using Godot;
using NobodyWho;
using NobodyWho.Enums;
using Shouldly;
using static GdUnit4.Assertions;

namespace CSharpIntegrationTests.Scripts.Tests;

[RequireGodotRuntime]
[TestSuite]
public class Embedding
{
    private NobodyWhoEmbedding _embedding;

    [Before]
    public void Setup()
    {
        using(ISceneRunner runner = ISceneRunner.Load("res://scenes/example.tscn"))
        {
            Node scene = AutoFree(runner.Scene());
            Node nobodyWhoEmbeddingNode = AutoFree(scene.GetNode("NobodyWhoEmbedding"));
            Node nobodyWhoModelNode = AutoFree(scene.GetNode("EmbeddingModel"));

            _embedding = new(nobodyWhoEmbeddingNode);
            _embedding.Model = new(nobodyWhoModelNode);
            _embedding.SetLogLevel(LogLevel.Trace);
        }
    }

    [TestCase]
    public async Task Test_Similarity()
    {
        // Generate some embeddings
        List<float> dragonHillEmbd = await _embedding.EmbedAsync("The dragon is on the hill.");
        List<float> dragonHungryEmbd = await _embedding.EmbedAsync("The dragon is hungry for humans.");
        List<float> irrelevantEmbd = await _embedding.EmbedAsync("This doesn't matter.");

        // Test similarity
        float lowSimilarity = _embedding.CosineSimilarity(irrelevantEmbd, dragonHillEmbd);
        float highSimilarity = _embedding.CosineSimilarity(dragonHillEmbd, dragonHungryEmbd);
        lowSimilarity.ShouldBeLessThan(highSimilarity);
    }
}