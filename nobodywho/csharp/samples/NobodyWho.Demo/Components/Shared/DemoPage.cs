using Microsoft.AspNetCore.Components;

namespace NobodyWho.Demo.Components.Shared;

/// <summary>
/// Base for the demo pages. Leaving a page while the model is still answering cancels
/// <see cref="Alive"/>, which stops generation, and then frees the page's chats.
/// </summary>
public abstract class DemoPage : ComponentBase, IDisposable
{
    private readonly CancellationTokenSource _alive = new();
    private readonly List<IDisposable> _owned = [];

    /// <summary>Cancelled when the visitor leaves the page.</summary>
    protected CancellationToken Alive => _alive.Token;

    protected bool IsDisposed => _alive.IsCancellationRequested;

    /// <summary>Free <paramref name="resource"/> when the page goes away.</summary>
    protected T Own<T>(T resource) where T : IDisposable
    {
        _owned.Add(resource);
        return resource;
    }

    /// <summary>
    /// Run an action from an event handler. Cancellation and use after the page has gone are the
    /// expected ways for work to end early, so they are not errors.
    /// </summary>
    protected async Task Guard(Func<Task> work)
    {
        try
        {
            await work();
        }
        catch (OperationCanceledException) when (IsDisposed)
        {
        }
        catch (ObjectDisposedException) when (IsDisposed)
        {
        }
    }

    protected virtual void OnDispose()
    {
    }

    public void Dispose()
    {
        _alive.Cancel();
        OnDispose();
        foreach (var resource in _owned)
            resource.Dispose();
        GC.SuppressFinalize(this);
    }
}
