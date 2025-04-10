using UnityEngine;
using UnityEngine.Events;
using System;
using System.Text;
using System.Runtime.InteropServices;

namespace NobodyWho {
    public class Embedding : MonoBehaviour {
        private IntPtr _actorContext;
        public Model model;
        
        public UnityEvent<float[]> onEmbeddingComplete = new UnityEvent<float[]>();
        public UnityEvent<string> onError = new UnityEvent<string>();

        private AwaitableCompletionSource<float[]> _embeddingSignal;
        private GCHandle _gcHandle;

        // we need a reference to the `OnEmbeddingCallback` to keep its pointer alive and not GC'ed. 
        // making a static variable solves this.
        private static NativeBindings.EmbeddingCallback _embeddingCallback = OnEmbeddingCallback;
        // This is a callback that is invoked when the embedding is complete generating.
        // the AOT (ahead of time compile ) is required for macos as their security model does not allow JIT.
        // the P/invoke marks this as a callback that can be called from native code to avoid it being optimized away.
        [AOT.MonoPInvokeCallback(typeof(NativeBindings.EmbeddingCallback))]
        private static void OnEmbeddingCallback(IntPtr caller, IntPtr data, int length) {
            GCHandle handle = GCHandle.FromIntPtr(caller);
            Embedding instance = handle.Target as Embedding;
            
            float[] embedding = new float[length];
            Marshal.Copy(data, embedding, 0, length);
            
            instance._embeddingSignal?.SetResult(embedding);
            instance.onEmbeddingComplete.Invoke(embedding);
        }

        void Start() {
            _gcHandle = GCHandle.Alloc(this);
            var errorBuffer = new StringBuilder(2048);
            
            _actorContext = NativeBindings.create_embedding_worker(
                model.GetModel(),
                GCHandle.ToIntPtr(_gcHandle),
                OnEmbeddingCallback,
                errorBuffer
            );
            
            if (errorBuffer.Length > 0) {
                Debug.LogError(errorBuffer.ToString());
                enabled = false;
            }
        }

        public Awaitable<float[]> Embed(string text) {
            Debug.Log("[DEBUG] Embedding: " + text);
            _embeddingSignal = new AwaitableCompletionSource<float[]>();
            
            var errorBuffer = new StringBuilder(2048);
            Debug.Log("[DEBUG] Embedding via: " + _actorContext);
            NativeBindings.embed_text(_actorContext, text, errorBuffer);
            
            if (errorBuffer.Length > 0) {
                Debug.LogError(errorBuffer.ToString());
            }
            
            return _embeddingSignal.Awaitable;
        }

        // This has several responsibilites:
        // 1. kill the embedding worker.
        // 2. kill the gc handle.
        // 3. deref the model strong count by 1.
        void OnDestroy() {
            if (_actorContext != IntPtr.Zero) {
                NativeBindings.destroy_embedding_worker(_actorContext);
            }
            
            if (_gcHandle.IsAllocated) {
                _gcHandle.Free();
            }
        }

        private void OnError(string error) {
            _embeddingSignal?.SetException(new NobodyWhoException(error));
            
            onError.Invoke(error);
            Debug.LogError($"Embedding Error: {error}");
        }

        public float CosineSimilarity(float[] a, float[] b) {
            return NativeBindings.CosineSimilarity(a, b);
        }
    }
} 