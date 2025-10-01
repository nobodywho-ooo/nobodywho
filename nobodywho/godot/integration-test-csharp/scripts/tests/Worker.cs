using System.Diagnostics;
using System.Threading.Tasks;
using GdUnit4;
using Godot;
using Shouldly;
using static GdUnit4.Assertions;

namespace CSharpIntegrationTests.Scripts.Tests;

[RequireGodotRuntime]
[TestSuite]
public class Worker
{
    private WorkerAsync _workerAsyncNode;

    [Before]
    public void Setup()
    {
        using(ISceneRunner runner = ISceneRunner.Load("res://scenes/example.tscn"))
        {
            Node scene = AutoFree(runner.Scene());

            _workerAsyncNode = AutoFree(scene.GetNode<WorkerAsync>("WorkerAsync"));
        }
    }

    [TestCase]
    public async Task Test_StartWorkerAsync()
    {
        Stopwatch worker1Stopwatch = await _workerAsyncNode.StartWorker1_DelayAsyncWait();

        Stopwatch worker2Stopwatch = await _workerAsyncNode.StartWorker2_DelaySyncWait();

        Stopwatch worker3Stopwatch = await _workerAsyncNode.StartWorker3_NoDelay();

        long worker1Elapsed = worker1Stopwatch.ElapsedTicks;
        long worker2Elapsed = worker2Stopwatch.ElapsedTicks;
        long worker3Elapsed = worker3Stopwatch.ElapsedTicks;

        worker3Elapsed.ShouldBeGreaterThan(worker1Elapsed);
        worker3Elapsed.ShouldBeGreaterThan(worker2Elapsed);
    }
}