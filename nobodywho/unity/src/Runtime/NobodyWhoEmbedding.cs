using UnityEngine;
using UnityEngine.Events;
using System;
using System.Text;
using System.Runtime.InteropServices;

namespace NobodyWho {
    public class Embedding : MonoBehaviour {
        private IntPtr _workerContext;
        public Model model;
        
        public UnityEvent<float[]> onEmbeddingComplete = new UnityEvent<float[]>();
        public UnityEvent<string> onError = new UnityEvent<string>();

        private AwaitableCompletionSource<float[]> _embeddingSignal;

        private void OnEmbedding(IntPtr data, int length) {
            float[] embedding = new float[length];
            Marshal.Copy(data, embedding, 0, length);
            
            _embeddingSignal?.SetResult(embedding);
            
            onEmbeddingComplete.Invoke(embedding);
        }

        private void OnError(string error) {
            _embeddingSignal?.SetException(new NobodyWhoException(error));
            
            onError.Invoke(error);
            Debug.LogError($"Embedding Error: {error}");
        }

        void Start() {
            try {
                var errorBuffer = new StringBuilder(2048);
                _workerContext = NativeBindings.create_embedding_worker(
                    model.GetModel(),
                    errorBuffer
                );
                
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        void Update() {
            if (_workerContext == IntPtr.Zero) {
                return;
            }
            try {
                NativeBindings.poll_embeddings(
                    _workerContext,
                    OnEmbedding,
                    OnError
                );
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        public Awaitable<float[]> Embed(string text) {
            try {
                var errorBuffer = new StringBuilder(2048);
                _embeddingSignal = new AwaitableCompletionSource<float[]>();
                
                NativeBindings.embed_text(_workerContext, text, errorBuffer);
                if (errorBuffer.Length > 0) {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }

                return _embeddingSignal.Awaitable;
            } catch (Exception e) {
                throw new NobodyWhoException(e.Message);
            }
        }

        public float CosineSimilarity(float[] a, float[] b) {
            return NativeBindings.CosineSimilarity(a, b);
        }
    }
} 