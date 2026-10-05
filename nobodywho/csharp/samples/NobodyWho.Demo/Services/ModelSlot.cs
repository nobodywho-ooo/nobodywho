namespace NobodyWho.Demo.Services;

public enum SlotState
{
    NotLoaded,
    Loading,
    Ready,
    Failed,
    NotConfigured,
}

/// <summary>Non-generic view of a <see cref="ModelSlot{T}"/>, for listing every model on one page.</summary>
public interface IModelSlot
{
    string Name { get; }
    string Purpose { get; }
    string Source { get; }
    SlotState State { get; }
    DownloadProgress? Progress { get; }
    string? Error { get; }
    TimeSpan? LoadTime { get; }
    event Action? Changed;
    Task LoadAsync();
}

/// <summary>
/// One model the whole app shares. It loads on first use, reports download progress, and can be
/// retried after a failure.
/// </summary>
public sealed class ModelSlot<T>(
    string name,
    string purpose,
    string source,
    Func<IProgress<DownloadProgress>, Task<T>> load) : IModelSlot, IDisposable
    where T : class, IDisposable
{
    private readonly Lock _lock = new();
    private Task<T>? _task;
    private long _lastNotify;

    public string Name { get; } = name;
    public string Purpose { get; } = purpose;
    public string Source { get; } = source;
    public SlotState State { get; private set; } = string.IsNullOrWhiteSpace(source) ? SlotState.NotConfigured : SlotState.NotLoaded;
    public DownloadProgress? Progress { get; private set; }
    public string? Error { get; private set; }
    public TimeSpan? LoadTime { get; private set; }
    public T? Value { get; private set; }

    public event Action? Changed;

    /// <summary>The loaded model, loading it first if needed.</summary>
    public Task<T> GetAsync()
    {
        if (State == SlotState.NotConfigured)
            throw new InvalidOperationException($"No {Name} model is configured in appsettings.json.");
        lock (_lock)
        {
            if (_task is null || _task.IsFaulted)
                _task = Run();
            return _task;
        }
    }

    Task IModelSlot.LoadAsync() => GetAsync();

    private async Task<T> Run()
    {
        State = SlotState.Loading;
        Error = null;
        Progress = null;
        Changed?.Invoke();
        var started = DateTime.UtcNow;
        try
        {
            Value = await Task.Run(() => load(new SlotProgress(this)));
            LoadTime = DateTime.UtcNow - started;
            State = SlotState.Ready;
            return Value;
        }
        catch (Exception e)
        {
            Error = e.Message;
            State = SlotState.Failed;
            throw;
        }
        finally
        {
            Changed?.Invoke();
        }
    }

    private void Report(DownloadProgress progress)
    {
        Progress = progress;
        // Downloads report about ten times a second; the page needs far fewer redraws.
        var now = Environment.TickCount64;
        if (now - Interlocked.Read(ref _lastNotify) > 250 || progress.Downloaded == progress.Total)
        {
            Interlocked.Exchange(ref _lastNotify, now);
            Changed?.Invoke();
        }
    }

    public void Dispose() => Value?.Dispose();

    private sealed class SlotProgress(ModelSlot<T> slot) : IProgress<DownloadProgress>
    {
        public void Report(DownloadProgress value) => slot.Report(value);
    }
}
