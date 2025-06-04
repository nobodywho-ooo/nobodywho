using System.Collections;
using System.Collections.Generic;
using NobodyWho;
using UnityEngine;
using UnityEngine.UIElements;

public class SimpleEmbeddingsController : MonoBehaviour
{
    [Header("References")]
    public Embedding embedding;

    public UIDocument uiDocument;
    private TextField inputText;
    private Button analyzeButton;
    private Label statusLabel;

    [Header("Predefined Phrases")]
    public string[] predefinedPhrases =
    {
        "Retrieve the ancient artifact of gaming",
        "Brave the Treacherous Dark Forest",
        "Seek the wisdom of the God of Cheese",
        "Gather ingredients for the feast of the Golden Harvest",
        "Unlock the mysteries of the NobodyWho",
    };

    private List<float[]> embeddedPhrases = new List<float[]>();
    private bool isPreEmbedding = true;

    void OnEnable()
    {
        var root = uiDocument.rootVisualElement;

        inputText = root.Q<TextField>("input-text");
        analyzeButton = root.Q<Button>("analyze-button");
        statusLabel = root.Q<Label>("status-label");
        // Update phrase labels to match predefinedPhrases array
        for (int i = 0; i < predefinedPhrases.Length; i++)
        {
            var phraseLabel = root.Q<Label>($"phrase-{i}");
            if (phraseLabel != null)
                phraseLabel.text = predefinedPhrases[i];
        }

        analyzeButton.clicked += AnalyzeSimilarity;
        embedding.StartWorker();
    }

    void Start()
    {
        PreEmbed();
    }

    void PreEmbed()
    {
        embeddedPhrases.Clear();
        isPreEmbedding = true;

        Debug.Log("Pre-embedding phrases: " + predefinedPhrases.Length);
        for (int i = 0; i < predefinedPhrases.Length; i++)
        {
            // Debug.Log("Pre-embedding nr:" + i + " phrase: " + predefinedPhrases[i]);

            embedding.Embed(predefinedPhrases[i]);
            float[] embeddingResult = embedding.GetEmbeddingBlocking();

            string embeddingResultString = string.Join(", ", embeddingResult);
            // Debug.Log("embeddingResult: " + embeddingResultString + " for phrase: " + predefinedPhrases[i]);
            embeddedPhrases.Add(embeddingResult);

            UpdateStatus($"Pre-embedded {i + 1}/{predefinedPhrases.Length}");
        }

        isPreEmbedding = false;
        // embedding.onEmbeddingComplete.AddListener(OnEmbeddingComplete);
        UpdateStatus("Ready!");
    }

    // void OnEmbeddingComplete(float[] result)
    // {
    //     Debug.Log("input result: " + string.Join(", ", result) + " for phrase: " + inputText.value);
    //     ClearSimilarityBars();
    //     for (int i = 0; i < embeddedPhrases.Count; i++)
    //     {
    //         float similarity = embedding.CosineSimilarity(result, embeddedPhrases[i]);
    //         Debug.Log($"Phrase {i}: '{predefinedPhrases[i]}' similarity={similarity}");
    //         UpdateSimilarityDisplay(i, similarity);
    //     }
    //     UpdateStatus("Done!");
    // }

    void AnalyzeSimilarity()
    {
        if (isPreEmbedding)
            return;
        if (string.IsNullOrWhiteSpace(inputText.value))
            return;
        if (embeddedPhrases.Count < predefinedPhrases.Length)
            return;
        embedding.Embed(inputText.value);
        float[] result = embedding.GetEmbeddingBlocking();
        Debug.Log("Phrase: " + inputText.value + " Result: " + string.Join(", ", result));
        for (int i = 0; i < embeddedPhrases.Count; i++)
        {
            float similarity = embedding.CosineSimilarity(result, embeddedPhrases[i]);
            // Debug.Log($"Phrase {i}: '{predefinedPhrases[i]}' similarity={similarity}");
            UpdateSimilarityDisplay(i, similarity);
        }
        UpdateStatus("Done!");
    }

    void ClearSimilarityBars()
    {
        var root = uiDocument.rootVisualElement;
        for (int i = 0; i < predefinedPhrases.Length; i++)
        {
            var fill = root.Q<VisualElement>($"similarity-fill-{i}");
            var score = root.Q<Label>($"similarity-score-{i}");
            if (fill != null)
                fill.style.width = Length.Percent(0);
            if (score != null)
                score.text = "0%";
        }
    }

    void UpdateSimilarityDisplay(int index, float similarity)
    {
        var root = uiDocument.rootVisualElement;
        var fill = root.Q<VisualElement>($"similarity-fill-{index}");
        var score = root.Q<Label>($"similarity-score-{index}");

        if (fill != null)
        {
            fill.style.width = Length.Percent(similarity * 100f);
            fill.style.backgroundColor = Color.Lerp(Color.red, Color.green, similarity);
        }
        if (score != null)
        {
            score.text = $"{similarity * 100f:F1}%";
        }
    }

    void UpdateStatus(string message)
    {
        if (statusLabel != null)
            statusLabel.text = $"Status: {message}";
    }
}
