using System.Collections.Generic;
using System.Threading.Tasks;
using GdUnit4;
using Godot;
using NobodyWho;
using NobodyWho.Enums;
using NobodyWho.Models;
using Shouldly;
using static GdUnit4.Assertions;

namespace CSharpIntegrationTests.Scripts.Tests;

[RequireGodotRuntime]
[TestSuite]
public class Chat
{
    private NobodyWhoChat _chat;

    [Before]
    public void Setup()
    {
        using(ISceneRunner runner = ISceneRunner.Load("res://scenes/example.tscn"))
        {
            Node scene = AutoFree(runner.Scene());
            Node nobodyWhoChatNode = AutoFree(scene.GetNode("NobodyWhoChat"));
            Node nobodyWhoModelNode = AutoFree(scene.GetNode("ChatModel"));

            _chat = new(nobodyWhoChatNode)
            {
                Model = new(nobodyWhoModelNode),
                SystemPrompt = "You are a helpful assistant, capable of answering questions about the world."
            };
            _chat.SetLogLevel(LogLevel.Trace);
        }
    }

    [TestCase]
    public async Task Test_Say()
    {
        _chat.Say("Please tell me what the capital city of Denmark is.");
        string response = await _chat.GetResponseAsync();

        response.ShouldContain("Copenhagen");
    }

    [TestCase]
    public async Task Test_Antiprompts()
    {
        _chat.StopWords = ["fly"];
        _chat.ResetContext(); // Restart the worker to include the antiprompts

        _chat.Say("List these animals in alphabetical order: cat, dog, fly, lion, mouse");
        string response = await _chat.GetResponseAsync();

        response.ShouldContain("dog", customMessage: "Should not stop before the antiprompt");
        response.ShouldContain("fly", customMessage: "Should reach the antiprompt");
        response.ShouldNotContain("lion", customMessage: "Should stop at antiprompt");
        response.ShouldNotContain("mouse", customMessage: "Should not continue past antiprompt");
    }

    [TestCase]
    public async Task Test_AntipromptsMultitokens()
    {
        _chat.StopWords = ["horse-rider"];
        _chat.SystemPrompt = "You only list the words in alphabetical order. nothing else.";
        _chat.ResetContext(); // Restart the worker to include the antiprompts

        _chat.Say("List all the words in alphabetical order: dog, horse-rider, lion, mouse");
        string response = await _chat.GetResponseAsync();

        response.ShouldContain("dog", customMessage: "Should not stop before the antiprompt");
        response.ShouldContain("horse-rider", customMessage: "Should reach the antiprompt");
        response.ShouldNotContain("lion", customMessage: "Should stop at antiprompt");
        response.ShouldNotContain("mouse", customMessage: "Should not continue past antiprompt");
    }

    [TestCase]
    public async Task Test_ChatHistory()
    {
        // Reset to clean slate.
        _chat.StopWords = [];
        _chat.ResetContext();

        _chat.SetChatHistory(
        [
            new(Role.User, "What is 2 + 2?"),
            new(Role.Assistant, "2 + 2 equals 4.")
        ]);

        List<ChatMessage> retrievedMessages = await _chat.GetChatHistoryAsync();

        // Basic validation.
        retrievedMessages.Count.ShouldBe(2, customMessage: "Should have 2 messages");
        retrievedMessages[0].Role.ShouldBe(Role.User, customMessage: "First message should be from user");
        retrievedMessages[0].Content.ShouldContain("2 + 2", customMessage:  "First message should be from user");
        retrievedMessages[1].Role.ShouldBe(Role.Assistant, customMessage: "First message should be from user");
        retrievedMessages[1].Content.ShouldContain("4", customMessage: "First message should be from user");

        _chat.Say("What did I just ask you about?");
        string response = await _chat.GetResponseAsync();
        response.ShouldContain("2 + 2");
    }

    [TestCase]
    public async Task Test_StopGeneration()
    {
        _chat.SystemPrompt = "You're countbot. A robot that's very good at counting";
        _chat.ResetContext();

        _chat.ResponseUpdated += ResponseUpdated;

        _chat.Say("count from 0 to 9");
        string response = await _chat.GetResponseAsync();
        
        response.ShouldContain("2");
        response.ShouldNotContain("8");

        List<ChatMessage> messages = await _chat.GetChatHistoryAsync();
        _chat.SetChatHistory(messages);
        List<ChatMessage> messagesAgain = await _chat.GetChatHistoryAsync();
        _chat.SetChatHistory(messagesAgain);

        messages.ShouldBeEquivalentTo(messagesAgain);

        _chat.ResponseUpdated -= ResponseUpdated;
    }

    [TestCase]
    public async Task Test_ToolCall()
    {
        Example exampleControl = _chat.ChatNode.GetParent<Example>();
        _chat.AddTool(exampleControl, nameof(exampleControl.CurrentTemperature), "Gets the current temperature in a given city.");
        _chat.SystemPrompt = "You're a helpful tool-calling assistant. Remember to keep proper tool calling syntax.";
        _chat.ResetContext();

        _chat.Say("I'd like to know the current temperature in Copenhagen.");
        string response = await _chat.GetResponseAsync();
        
        response.ShouldContain("12.34");
    }

    private void ResponseUpdated(string token)
    {
        if(token.Contains('2'))
        {
            _chat.StopGeneration();
        }
    }
}