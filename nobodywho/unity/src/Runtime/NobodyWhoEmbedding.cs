using System;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;
using UnityEngine.Events;

namespace NobodyWho
{
    public class Embedding : MonoBehaviour
    {
        public EmbedWrapper wrapper = EmbedWrapper.New();

        [Header("Configuration")]
        public Model model;
        public uint contextLength = 4096;

        [Header("Events")]
        public UnityEvent<float[]> onEmbeddingComplete = new UnityEvent<float[]>();

        public void Embed(string text)
        {
            wrapper.Embed(text);
        }

        void Update()
        {
            var resultslice = wrapper.PollEmbedding();
            if (resultslice.Count > 0)
            {
                var embd = resultslice.Copied;
                onEmbeddingComplete.Invoke(embd);
            }
        }

        public void StartWorker()
        {
            wrapper.StartWorker(model.modelWrapperContext, contextLength);
        }

        public float CosineSimilarity(float[] a, float[] b)
        {
            // Ugh.. clearly there's something I'm misunderstanding here
            // This is the only place in the entire project where I have to do manual alloc now
            // TODO: understand interoptopus slice passing better.
            GCHandle pina = GCHandle.Alloc(a, GCHandleType.Pinned);
            GCHandle pinb = GCHandle.Alloc(b, GCHandleType.Pinned);
            try
            {
                var slicea = new Slicef32(pina, (ulong)a.Length);
                var sliceb = new Slicef32(pinb, (ulong)b.Length);

                // call the exported Rust function
                return NobodyWhoBindings.cosine_similarity(slicea, sliceb);
            }
            finally
            {
                pina.Free();           // always un-pin!
                pinb.Free();           // always un-pin!
            }
        }

        public float[] GetEmbeddingBlocking()
        {
            // this is only really used in tests.
            // it blocks forever, or until a finished response is emitted
            // TODO: figure out a nicer async API
            while (true)
            {
                var resultslice = wrapper.PollEmbedding();
                if (resultslice.Count > 0)
                {
                    var embd = resultslice.Copied;
                    return embd;
                }
                System.Threading.Thread.Sleep(10);
            }
        }

        private void OnDestroy()
        {
            if (wrapper != null)
            {
                wrapper.Dispose();
                wrapper = null;
            }
        }
    }
}
