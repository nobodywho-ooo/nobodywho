using System.Diagnostics;
using System.Threading.Tasks;
using Godot;
using NobodyWho;

namespace CSharpIntegrationTests.Scripts;

public partial class WorkerAsync : Node
{
    private NobodyWhoChat _chat1;
    private NobodyWhoChat _chat2;
    private NobodyWhoChat _chat3;

    public override void _Ready()
    {
        _chat1 = new(GetNode("NobodyWhoChat1"));
        _chat1.Model = new(GetNode("NobodyWhoModel1"));

        _chat2 = new(GetNode("NobodyWhoChat2"));
        _chat2.Model = new(GetNode("NobodyWhoModel2"));

        _chat3 = new(GetNode("NobodyWhoChat3"));
        _chat3.Model = new(GetNode("NobodyWhoModel3"));
    }

    public async Task<Stopwatch> StartWorker1_DelayAsyncWait()
    {
        Task startWorkerTask = _chat1.StartWorkerAsync();

        await Task.Delay(5000); // Simulate some stuff happening.

        Stopwatch stopwatch = Stopwatch.StartNew();
        await startWorkerTask;
        stopwatch.Stop();

        return stopwatch;
    }

    public async Task<Stopwatch> StartWorker2_DelaySyncWait()
    {
        Task startWorkerTask = _chat2.StartWorkerAsync();

        Task.Delay(5000).Wait(); // Simulate some stuff happening (blocking).

        Stopwatch stopwatch = Stopwatch.StartNew();
        await startWorkerTask;
        stopwatch.Stop();

        return stopwatch;
    }

    public async Task<Stopwatch> StartWorker3_NoDelay()
    {
        Stopwatch stopwatch = Stopwatch.StartNew();
        await _chat3.StartWorkerAsync();
        stopwatch.Stop();

        return stopwatch;
    }
}