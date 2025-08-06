using Godot;

namespace CSharpIntegrationTests.Scripts;

public partial class Example : Control
{
    public string CurrentTemperature(string location)
    {
        if(string.Equals(location.ToLowerInvariant(), "copenhagen"))
        {
            return "12.34";
        }

        return "Unknown city name";
    }
}