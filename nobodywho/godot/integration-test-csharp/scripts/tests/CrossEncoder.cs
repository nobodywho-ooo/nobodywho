using System;
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
public class CrossEncoder
{
    private NobodyWhoCrossEncoder _crossEncoder;

    [Before]
    public void Setup()
    {
        using(ISceneRunner runner = ISceneRunner.Load("res://scenes/example.tscn"))
        {
            Node scene = AutoFree(runner.Scene());
            Node nobodyWhoCrossEncoderNode = AutoFree(scene.GetNode("CrossEncoder"));
            Node nobodyWhoModelNode = AutoFree(scene.GetNode("CrossEncoderModel"));

            _crossEncoder = new(nobodyWhoCrossEncoderNode)
            {
                Model = new(nobodyWhoModelNode)
            };

            _crossEncoder.SetLogLevel(LogLevel.Trace);
            // ^ For some reason any other log level causes an error "Illegal log level to be called here"
            // \.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\llama-cpp-2-0.1.112\src\log.rs:95
        }
    }

    [TestCase]
    public async Task Test_RankDocuments()
    {
        string query = "What is the capital of France?";
        List<string> documents = [
            "The Eiffel Tower is a famous landmark in the capital of France.",
            "France is a country in Europe.",
            "Lyon is a major city in France, but not the capital.",
            "The capital of Germany is France.",
            "The French government is based in Paris.",
            "France's capital city is known for its art and culture, it is called Paris.",
            "The Louvre Museum is located in Paris, France - which is the largest city, and the seat of the government",
            "Paris is the capital of France.",
            "Paris is not the capital of France.",
            "The president of France works in Paris, the main city of his country.",
            "What is the capital of France?"
        ];

        List<string> rankedDocs = await _crossEncoder.RankAsync(query, documents, limit: 3);

        rankedDocs.Count.ShouldBe(3, customMessage: "Should return exactly 3 documents");
        string.Join(string.Empty, rankedDocs).ShouldContain("Paris is the capital of France",
            customMessage: "Paris is the capital of France should be in the top 3");

        List<string> allRankedDocs = await _crossEncoder.RankAsync(query, documents, limit: -1);
        allRankedDocs.Count.ShouldBe(documents.Count, "Should return all documents when limit is -1");
    }
}