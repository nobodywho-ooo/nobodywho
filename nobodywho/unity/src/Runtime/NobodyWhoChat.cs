using System;
using System.Collections;
using System.Runtime.InteropServices;
using System.Text;
using UnityEngine;
using UnityEngine.Events;

namespace NobodyWho
{
    public class Chat : MonoBehaviour
    {
        public Model model;

        [TextArea(15, 20)]
        public string systemPrompt = "";

        [Header("Configuration")]
        public string stopWords = "";
        public int contextLength = 4096;
        public bool use_grammar = false;

        [TextArea(15, 20)]
        public string grammar;

        [Header("Events")]
        public UnityEvent<string> onToken = new UnityEvent<string>();
        public UnityEvent<string> onComplete = new UnityEvent<string>();

        private AwaitableCompletionSource<string> _completionSignal;
        private AwaitableCompletionSource<string> _tokenSignal;
        private IntPtr _workerContext;

        // We need a reference to the `OnEmbeddingCallback` to keep its pointer alive and not GC'ed.
        private GCHandle _gcHandle;

        // Static callbacks to prevent garbage collection
        private static NativeBindings.ChatTokenCallback _tokenCallback = OnTokenCallback;
        private static NativeBindings.ChatCompletionCallback _completionCallback =
            OnCompletionCallback;
        private static NativeBindings.ChatErrorCallback _errorCallback = OnErrorCallback;

        // This is a callback that is invoked when the embedding is complete generating.
        // the AOT (ahead of time compile) is required for ios as their security model does not allow JIT. https://stackoverflow.com/questions/5054732/is-it-prohibited-using-of-jitjust-in-time-compiled-code-in-ios-app-for-appstor
        // the P/invoke marks this as a callback that can be called from native code to avoid it being optimized away.
        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatTokenCallback))]
        private static void OnTokenCallback(IntPtr caller, IntPtr tokenPtr)
        {
            GCHandle handle = GCHandle.FromIntPtr(caller);
            Chat instance = handle.Target as Chat;

            string token = Marshal.PtrToStringUTF8(tokenPtr);

            instance.onToken.Invoke(token);
            instance._tokenSignal?.SetResult(token);
        }

        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatCompletionCallback))]
        private static void OnCompletionCallback(IntPtr caller, IntPtr responsePtr)
        {
            GCHandle handle = GCHandle.FromIntPtr(caller);
            Chat instance = handle.Target as Chat;

            string response = Marshal.PtrToStringUTF8(responsePtr);
            instance.onComplete.Invoke(response);
            instance._completionSignal?.SetResult(response);
        }

        [AOT.MonoPInvokeCallback(typeof(NativeBindings.ChatErrorCallback))]
        private static void OnErrorCallback(IntPtr caller, IntPtr errorPtr)
        {
            GCHandle handle = GCHandle.FromIntPtr(caller);
            Chat instance = handle.Target as Chat;

            string error = Marshal.PtrToStringUTF8(errorPtr);
            Debug.LogError($"Error while generating response: {error}");
            instance.onComplete.Invoke(error);
            instance._completionSignal?.SetException(new NobodyWhoException(error));
        }

        void Start()
        {
            try
            {
                _gcHandle = GCHandle.Alloc(this);
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                // Todo - check if there is a builtin setter and getter that atoconverts to and from a string/string-array
                var stopWordsString = "";
                if (stopWords.Length > 0)
                {
                    stopWordsString = string.Join(",", stopWords);
                }
                _workerContext = NativeBindings.create_chat_worker(
                    model.GetModel(),
                    systemPrompt,
                    stopWordsString,
                    contextLength,
                    use_grammar,
                    grammar,
                    GCHandle.ToIntPtr(_gcHandle),
                    _tokenCallback,
                    _completionCallback,
                    _errorCallback,
                    errorBuffer
                );

                if (errorBuffer.Length > 0)
                {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }
            }
            catch (Exception e)
            {
                throw new NobodyWhoException(e.Message);
            }
        }

        void OnDestroy()
        {
            // Free the GCHandle
            if (_gcHandle.IsAllocated)
            {
                _gcHandle.Free();
            }

            NativeBindings.destroy_chat_worker(_workerContext);
        }

        // This deletes the old worker context and creates a new one with the new params, it also means that we lose the chat history
        public void ResetContext()
        {
            NativeBindings.destroy_chat_worker(_workerContext);
            var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
            _workerContext = NativeBindings.create_chat_worker(
                model.GetModel(),
                systemPrompt,
                stopWords,
                contextLength,
                use_grammar,
                grammar,
                GCHandle.ToIntPtr(_gcHandle),
                _tokenCallback,
                _completionCallback,
                _errorCallback,
                errorBuffer
            );
            if (errorBuffer.Length > 0)
            {
                throw new NobodyWhoException(errorBuffer.ToString());
            }
        }

        public Awaitable<string> Say(string prompt)
        {
            try
            {
                var errorBuffer = new StringBuilder(2048); // update lib.rs if you change this value
                _completionSignal = new AwaitableCompletionSource<string>();

                NativeBindings.send_prompt(_workerContext, prompt, errorBuffer);
                if (errorBuffer.Length > 0)
                {
                    throw new NobodyWhoException(errorBuffer.ToString());
                }

                return _completionSignal.Awaitable;
            }
            catch (Exception e)
            {
                throw new NobodyWhoException(e.Message);
            }
        }
    }
}
