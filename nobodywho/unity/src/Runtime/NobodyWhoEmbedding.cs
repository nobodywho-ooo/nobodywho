using System;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;
using UnityEngine.Events;

namespace NobodyWho
{
    public class Embedding : MonoBehaviour
    {
        private IntPtr _actorContext;
        public Model model;

        public UnityEvent<float[]> onEmbeddingComplete = new UnityEvent<float[]>();
        public UnityEvent<string> onError = new UnityEvent<string>();

        private AwaitableCompletionSource<float[]> _embeddingSignal;
        private GCHandle _gcHandle;

        void Start()
        {
            _gcHandle = GCHandle.Alloc(this);
            var errorBuffer = new StringBuilder(2048);

            _actorContext = NativeBindings.create_embedding_worker(
                model.GetModel(),
                errorBuffer
            );

            if (errorBuffer.Length > 0)
            {
                Debug.LogError(errorBuffer.ToString());
                enabled = false;
            }
        }

        public Awaitable<float[]> Embed(string text)
        {
            _embeddingSignal = new AwaitableCompletionSource<float[]>();

            var errorBuffer = new StringBuilder(2048);
            NativeBindings.embed_text(_actorContext, text, errorBuffer);

            if (errorBuffer.Length > 0)
            {
                Debug.LogError(errorBuffer.ToString());
                // TODO: throw exception here
            }
            return _embeddingSignal.Awaitable;
        }

        void Update() {
            float[] embd = NativeBindings.PollEmbeddings(_actorContext);
            if (embd != null) {
                onEmbeddingComplete.Invoke(embd);
                _embeddingSignal?.SetResult(embd);
            }
            // TODO: why do we have both an AwaitableCompletionSource and a UnityEvent?
            //       could we get away with having only one?
        }

        // This has several responsibilites:
        // 1. kill the embedding worker.
        // 2. kill the gc handle.
        // 3. deref the model strong count by 1.
        void OnDestroy()
        {
            if (_actorContext != IntPtr.Zero)
            {
                NativeBindings.destroy_embedding_worker(_actorContext);
            }

            if (_gcHandle.IsAllocated)
            {
                _gcHandle.Free();
            }
        }

        private void OnError(string error)
        {
            _embeddingSignal?.SetException(new NobodyWhoException(error));

            onError.Invoke(error);
        }

        public float CosineSimilarity(float[] a, float[] b)
        {
            return NativeBindings.CosineSimilarity(a, b);
        }
    }
}
