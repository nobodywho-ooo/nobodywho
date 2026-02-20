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
public class Grammar
{
    private NobodyWhoChat _chat;

    [Before]
    public void Setup()
    {
        using(ISceneRunner runner = ISceneRunner.Load("res://scenes/example.tscn"))
        {
            Node scene = AutoFree(runner.Scene());
            Node grammarScene = AutoFree(scene.GetNode("Grammar"));
            Node nobodyWhoChatNode = AutoFree(grammarScene.GetNode("Chat"));
            Node nobodyWhoModelNode = AutoFree(grammarScene.GetNode("Model"));

            _chat = new(nobodyWhoChatNode)
            {
                Model = new(nobodyWhoModelNode),
                SystemPrompt = "You are a character creator for a fantasy game. You will be given a list of properties and you will need to fill out those properties."
            };
            _chat.SetLogLevel(LogLevel.Trace);
            // ^ For some reason any other log level causes an error "Illegal log level to be called here"
            // \.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\llama-cpp-2-0.1.112\src\log.rs:95

            _chat.Sampler = NobodyWhoSampler.Create();
            _chat.Sampler.UseGrammar = true;
            AutoFree(_chat.Sampler.SamplerResource);

            // I used this webapp to make a gbnf from a json schema
            // https://adrienbrault.github.io/json-schema-to-gbnf/
            // XXX: needed to :%s/\\/\\\\/g afterwards to escape the backslashes
            _chat.Sampler.GbnfGrammar = @"root ::= ""{"" ws01 root-name "","" ws01 root-class "","" ws01 root-level ""}"" ws01
root-name ::= ""\""name\"""" "":"" ws01 string
root-class ::= ""\""class\"""" "":"" ws01 (""\""fighter\"""" | ""\""ranger\"""" | ""\""wizard\"""")
root-level ::= ""\""level\"""" "":"" ws01 integer


value  ::= (object | array | string | number | boolean | null) ws

object ::=
  ""{"" ws (
	string "":"" ws value
	("","" ws string "":"" ws value)*
  )? ""}""

array  ::=
  ""["" ws01 (
			value
	("","" ws01 value)*
  )? ""]""

string ::=
  ""\"""" (string-char)* ""\""""

string-char ::= [^""\\] | ""\\"" ([""\\/bfnrt] | ""u"" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F]) # escapes

number ::= integer (""."" [0-9]+)? ([eE] [-+]? [0-9]+)?
integer ::= ""-""? ([0-9] | [1-9] [0-9]*)
boolean ::= ""true"" | ""false""
null ::= ""null""

# Optional space: by convention, applied in this grammar after literal chars when allowed
ws ::= ([ \t\n] ws)?
ws01 ::= ([ \t\n])?";
        }
    }

    [TestCase]
    public async Task Test_JsonOutput()
    {
        // purposefully not mentioning the grammar type in the system prompt
        _chat.Say(@"Generate exactly these properties:
	- name
	- class
	- level");

        string response = await _chat.GetResponseAsync();

        Json json = new();
        Error error = json.Parse(response);
        error.ShouldBe(Error.Ok, customMessage: $"Invalid JSON received.Parse error at line {json.GetErrorLine()}: {json.GetErrorMessage()}");

        Godot.Collections.Dictionary data = json.Data.AsGodotDictionary();

        data.ShouldContainKey("name");
        data.ShouldContainKey("class");
        data.ShouldContainKey("level");
    }
}