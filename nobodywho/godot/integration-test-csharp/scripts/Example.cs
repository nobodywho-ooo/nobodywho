using Godot;

namespace CSharpIntegrationTests.Scripts;

public partial class Example : Control
{
    public bool ToolCalled { get; set; } = false;

    public string CurrentTemperature(string location, int zipCode, bool inDenmark)
    {
        if(string.Equals(location.ToLowerInvariant(), "copenhagen"))
        {
            return "12.34";
        }

        return "Unknown city name";
    }

    public string CallTool()
    {
        ToolCalled = true;

        return "flag set";
    }
}